use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RABBITMQ_HIGH_PRIORITY},
    riot::games::{
        VALORANT_SHORTHAND,
        valorant::{
            ValorantMatchlistDto,
            ValorantMatchDto
        }
    }
};
use reqwest::{StatusCode};
use super::RiotApiTask;
use crate::riot::db;

impl super::RiotApiHandler {
    pub async fn get_valorant_matches_for_user(&self, puuid: &str, shard: &str) -> Result<ValorantMatchlistDto, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(shard, &format!("val/match/v1/matchlists/by-puuid/{}", puuid));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain Valorant matches for user {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<ValorantMatchlistDto>().await?)
    }

    pub async fn get_valorant_match(&self, match_id: &str, shard: &str) -> Result<ValorantMatchDto, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(shard, &format!("val/match/v1/matches/{}", match_id));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain Valorant match {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<ValorantMatchDto>().await?)
    }
}

impl super::RiotApiApplicationInterface {
    pub async fn backfill_user_valorant_matches(&self, puuid: &str) -> Result<(), SquadOvError> {
        // Refresh the user's active shard whenever they request a backfill (this corresponds to when
        // they launch the game so it should work nicely).SquadOvError
        let shard = self.api.get_active_shard_by_game_for_puuid(VALORANT_SHORTHAND, puuid).await?;
        db::set_user_account_shard(&*self.db, puuid, VALORANT_SHORTHAND, &shard).await?;

        // Obtain a list of matches that the user played from the VALORANT API and then cross check that
        // with the matches we have stored. If the match doesn't exist then go ahead and request a low
        // priority match retrieval for that particular match.
        let api_matches = self.api.get_valorant_matches_for_user(puuid, &shard).await?;
        let match_ids: Vec<String> = api_matches.history.into_iter().map(|x| { x.match_id }).collect();
        let backfill_ids = db::get_valorant_matches_that_require_backfill(&*self.db, &match_ids).await?;
        for mid in &backfill_ids {
            self.request_obtain_valorant_match_info(&mid, &shard, false).await?;
        }
        Ok(())
    }

    pub async fn request_backfill_user_valorant_matches(&self, puuid: &str) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.queue, serde_json::to_vec(&RiotApiTask::ValorantBackfill{puuid: String::from(puuid)})?, RABBITMQ_DEFAULT_PRIORITY).await;
        Ok(())
    }

    pub async fn obtain_valorant_match_info(&self, match_id: &str, shard: &str) -> Result<(), SquadOvError> {
        // Check to make sure that we haven't already retrieved match details for this particular match.
        // This case could happen when multiple SquadOV users are in the same match and thus all submit
        // request for obtaining match details at the same time. Since this process is currently effectively single
        // threaded (i.e. we only have one request for information at a time), we can just perform a simple
        // query against the database to see if the match details exist or not.
        // TODO: Is there a way to make this check 100% reliable (or close to it) when we have multiple threads?
        if db::check_valorant_match_details_exist(&*self.db, match_id).await? {
            return Ok(());
        }

        let valorant_match = self.api.get_valorant_match(match_id, shard).await?;

        // There are two cases here: either 1) we're coming from the user created match endpoint in which case a match UUID already probably exists
        // or 2) we're coming from the backfill where the match UUID doesn't exist. We need to handle case #2 by creating the match UUID.
        for _i in 0..2i32 {
            let mut tx = self.db.begin().await?;
            let match_uuid = match db::create_or_get_match_uuid_for_valorant_match(&mut tx, match_id).await {
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
            db::store_valorant_match_dto(&mut tx, &match_uuid, &valorant_match).await?;
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

        self.rmq.publish(&self.queue, serde_json::to_vec(&RiotApiTask::ValorantMatch{
            match_id: String::from(match_id),
            shard: String::from(shard),
        })?, priority).await;
        Ok(())
    }
}