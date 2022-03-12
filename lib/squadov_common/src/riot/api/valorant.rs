use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RABBITMQ_HIGH_PRIORITY},
    riot::games::{
        VALORANT_SHORTHAND,
        valorant::{
            ValorantMatchlistDto,
            ValorantMatchDto,
        }
    }
};
use super::RiotApiTask;
use crate::riot::db;
use uuid::Uuid;

const RIOT_MAX_AGE_SECONDS: i64 = 172800; // 2 day

impl super::RiotApiHandler {
    pub async fn get_valorant_matches_for_user(&self, puuid: &str, shard: &str) -> Result<ValorantMatchlistDto, SquadOvError> {
        self.check_region_status("val", shard, false).await?;

        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(shard, &format!("val/match/v1/matchlists/by-puuid/{}", puuid));
        self.tick_thresholds().await?;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        let resp = self.check_for_response_error(resp, "Failed to obtain Valorant matches for user").await?;
        Ok(resp.json::<ValorantMatchlistDto>().await?)
    }

    pub async fn get_valorant_match(&self, match_id: &str, shard: &str, allow_failover: bool) -> Result<ValorantMatchDto, SquadOvError> {
        self.check_region_status("val", shard, allow_failover).await?;

        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(shard, &format!("val/match/v1/matches/{}", match_id));
        self.tick_thresholds().await?;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        let resp = self.check_for_response_error(resp, "Failed to obtain Valorant match").await?;
        Ok(resp.json::<ValorantMatchDto>().await?)
    }
}

impl super::RiotApiApplicationInterface {
    pub async fn backfill_user_valorant_matches(&self, puuid: &str) -> Result<(), SquadOvError> {
        // Refresh the user's active shard whenever they request a backfill (this corresponds to when
        // they launch the game so it should work nicely).
        let shard = self.api.get_active_shard_by_game_for_puuid(VALORANT_SHORTHAND, puuid).await?;
        db::set_user_account_shard(&*self.db, puuid, VALORANT_SHORTHAND, &shard).await?;

        log::info!("Requesting Valorant Backfill: {} [{}]", puuid, shard);

        // Disable backfill for now until we have a better idea on how to process more matches quickly.
        /*
        // Obtain a list of matches that the user played from the VALORANT API and then cross check that
        // with the matches we have stored. If the match doesn't exist then go ahead and request a low
        // priority match retrieval for that particular match.
        let api_matches = self.api.get_valorant_matches_for_user(puuid, &shard).await?;
        let match_ids: Vec<String> = api_matches.history.into_iter().map(|x| { x.match_id }).collect();
        let backfill_ids = db::get_valorant_matches_that_require_backfill(&*self.db, &match_ids).await?;
        for mid in &backfill_ids {
            self.request_obtain_valorant_match_info(&mid, &shard, false).await?;
        }
        */
        Ok(())
    }

    pub async fn request_backfill_user_valorant_matches(&self, puuid: &str) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.valorant_queue, serde_json::to_vec(&RiotApiTask::ValorantBackfill{puuid: String::from(puuid)})?, RABBITMQ_DEFAULT_PRIORITY, RIOT_MAX_AGE_SECONDS).await;
        Ok(())
    }

    pub async fn obtain_valorant_match_info(&self, match_id: &str, shard: &str, allow_failover: bool) -> Result<(), SquadOvError> {
        // Check to make sure that we haven't already retrieved match details for this particular match.
        // This case could happen when multiple SquadOV users are in the same match and thus all submit
        // request for obtaining match details at the same time. Since this process is currently effectively single
        // threaded (i.e. we only have one request for information at a time), we can just perform a simple
        // query against the database to see if the match details exist or not.
        // TODO: Is there a way to make this check 100% reliable (or close to it) when we have multiple threads?
        if db::check_valorant_match_details_exist(&*self.db, match_id).await? {
            return Ok(());
        }
        log::info!("Obtaining Valorant Match Details: {} [{}]", match_id, shard);

        let valorant_match = self.api.get_valorant_match(match_id, shard, allow_failover).await?;

        // There are two cases here: either 1) we're coming from the user created match endpoint in which case a match UUID already probably exists
        // or 2) we're coming from the backfill where the match UUID doesn't exist. We need to handle case #2 by creating the match UUID.
        for _i in 0..2i32 {
            let mut tx = self.db.begin().await?;
            log::info!("...Create or Get Val Match Uuid: {} [{}]", match_id, shard);
            let match_uuid = match db::create_or_get_match_uuid_for_valorant_match(&mut tx, match_id, shard).await {
                Ok(x) => x,
                Err(err) => match err {
                    SquadOvError::Duplicate => {
                        // This indicates that the match UUID is INVALID because a match with the same
                        // match ID already exists. Retry!
                        log::warn!("Caught duplicate Valorant match {} [{}]...retrying!", match_id, shard);
                        tx.rollback().await?;
                        continue;
                    },
                    _ => return Err(err)
                }
            };

            log::info!("...Store Val Match DTO: {} [{}] - {}", match_id, shard, &match_uuid);
            db::store_valorant_match_dto(&mut tx, &match_uuid, &valorant_match).await?;

            log::info!("...Cache Val Match Info: {} [{}] - {}", match_id, shard, &match_uuid);
            db::cache_valorant_match_information(&mut tx, &match_uuid).await?;

            // Cache POV information for each player already registered to squadov
            // We do this for users who have their accounts linked already because we need to cache relative to user ids.
            // We'll fill in that information for other users when/if they ever link their account.
            log::info!("...Get SquadOV Users in Val Match: {} [{}] - {}", match_id, shard, &match_uuid);
            let match_user_ids = db::get_squadov_user_ids_in_valorant_match(&mut tx, &match_uuid).await?;
            for user_id in match_user_ids {
                log::info!("...Cache Val Match POV: {} [{}] - User ID: {} - {}", match_id, shard, user_id, &match_uuid);
                db::cache_valorant_player_pov_information(&mut tx, &match_uuid, user_id).await?;
            }

            tx.commit().await?;
            break;
        }
        Ok(())
    }

    pub async fn request_obtain_valorant_match_info(&self, match_id: &str, shard: &str, priority: bool) -> Result<(), SquadOvError> {
        let priority = if priority {
            RABBITMQ_HIGH_PRIORITY
        } else {
            RABBITMQ_DEFAULT_PRIORITY
        };

        self.rmq.publish(&self.mqconfig.valorant_queue, serde_json::to_vec(&RiotApiTask::ValorantMatch{
            match_id: String::from(match_id),
            shard: String::from(shard),
        })?, priority, RIOT_MAX_AGE_SECONDS).await;
        Ok(())
    }

    pub async fn request_valorant_match_player_cache_data(&self, match_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.misc_valorant_queue, serde_json::to_vec(&RiotApiTask::ValorantMatchPlayerCacheData{
            match_uuid: match_uuid.clone(),
            user_id,
        })?, RABBITMQ_DEFAULT_PRIORITY, RIOT_MAX_AGE_SECONDS).await;
        Ok(())
    }
}