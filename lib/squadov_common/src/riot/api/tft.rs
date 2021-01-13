use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RABBITMQ_HIGH_PRIORITY},
    riot::{
        db,
        games::{
            TFT_SHORTHAND,
            TftMatchDto
        },
    }
};
use super::RiotApiTask;
use chrono::{Utc, Duration};
use reqwest::{StatusCode};

impl super::RiotApiHandler {
    pub async fn get_tft_matches_for_user(&self, puuid: &str, region: &str, count: i32) -> Result<Vec<String>, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(&super::riot_region_to_routing(region)?, &format!("tft/match/v1/matches/by-puuid/{}/ids?count={}", puuid, count));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain TFT matches for user {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<Vec<String>>().await?)
    }

    pub async fn get_tft_match(&self, region: &str, match_id: &str) -> Result<TftMatchDto, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint(&super::riot_region_to_routing(region)?, &format!("tft/match/v1/matches/{}", match_id));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain TFT match {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<TftMatchDto>().await?)
    }
}

const TFT_BACKFILL_AMOUNT: i32 = 100;

impl super::RiotApiApplicationInterface {
    pub async fn request_obtain_tft_match_info(&self, platform: &str, region: &str, game_id: i64, priority: bool) -> Result<(), SquadOvError> {
        let priority = if priority {
            RABBITMQ_HIGH_PRIORITY
        } else {
            RABBITMQ_DEFAULT_PRIORITY
        };

        self.rmq.publish(&self.queue, serde_json::to_vec(&RiotApiTask::TftMatch{
            platform: String::from(platform),
            region: String::from(region),
            game_id,
        })?, priority).await;
        Ok(())
    }

    pub async fn obtain_tft_match_info(&self, platform: &str, region: &str, game_id: i64) -> Result<(), SquadOvError> {
        let tft_match = self.api.get_tft_match(region, &format!(
            "{}_{}",
            &platform.to_uppercase(),
            game_id,
        )).await?;

        // We also need to obtain info about every player in the match to get their names since the TFT endpoint doesn't provide that info
        // off the bat and only provides PUUIDs.
        let puuids: Vec<String> = tft_match.info.participants.iter().map(|x| { x.puuid.clone() }).collect();
        let missing_puuids = db::get_missing_riot_account_puuids(&*self.db, &puuids, TFT_SHORTHAND).await?;
        for id in &missing_puuids {
            let mut tx = self.db.begin().await?;
            let summoner = self.api.get_tft_summoner_from_puuid(id, platform).await?;
            db::store_riot_summoner(&mut tx, &summoner, TFT_SHORTHAND).await?;
            tx.commit().await?;
        }

        for _i in 0..2i32 {
            let mut tx = self.db.begin().await?;
            let match_uuid = match db::create_or_get_match_uuid_for_tft_match(&mut tx, platform, region, game_id).await {
                Ok(x) => x,
                Err(err) => match err {
                    SquadOvError::Duplicate => {
                        log::warn!("Caught duplicate TFT match...retrying!");
                        tx.rollback().await?;
                        continue;
                    },
                    _ => return Err(err)
                }
            };
            db::store_tft_match_info(&mut tx, &match_uuid, &tft_match).await?;
            tx.commit().await?;
            break;
        }

        Ok(())
    }

    pub async fn request_backfill_user_tft_matches(&self, summoner_name: &str, region: &str, user_id: i64) -> Result<(), SquadOvError> {
        let summoner = db::get_user_riot_summoner_from_name(&*self.db, user_id, summoner_name, &self.game).await?;
        if summoner.last_backfill_time.is_some() {
            let time_since_backfill = Utc::now() - summoner.last_backfill_time.unwrap();
            if time_since_backfill > Duration::days(3) {
                return Ok(());
            }
        }

        self.rmq.publish(&self.queue, serde_json::to_vec(&RiotApiTask::TftBackfill{
            puuid: summoner.puuid.clone(),
            region: String::from(region),
        })?, RABBITMQ_DEFAULT_PRIORITY).await;
        Ok(())
    }

    pub async fn backfill_user_tft_matches(&self, puuid: &str, region: &str) -> Result<(), SquadOvError> {
        let match_ids = self.api.get_tft_matches_for_user(puuid, region, TFT_BACKFILL_AMOUNT).await?;
        db::tick_riot_puuid_backfill_time(&*self.db, puuid, TFT_SHORTHAND).await?;

        let backfill_ids = db::get_tft_matches_that_require_backfill(&*self.db, &match_ids).await?;
        for (platform, game_id) in &backfill_ids {
            self.request_obtain_tft_match_info(&platform, region, *game_id, false).await?;
        }
        Ok(())
    }
}