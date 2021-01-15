use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY},
    riot::RiotAccount
};
use super::RiotApiTask;
use reqwest::{StatusCode};
use crate::riot::db;
use serde::Deserialize;

impl super::RiotApiHandler {
    pub async fn get_account_by_puuid(&self, puuid: &str) -> Result<RiotAccount, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint("americas", &format!("riot/account/v1/accounts/by-puuid/{}", puuid));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to obtain Riot acount by PUUID {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        Ok(resp.json::<RiotAccount>().await?)
    }

    pub async fn get_active_shard_by_game_for_puuid(&self, game: &str, puuid: &str) -> Result<String, SquadOvError> {
        let client = self.create_http_client()?;
        let endpoint = Self::build_api_endpoint("americas", &format!("riot/account/v1/active-shards/by-game/{game}/by-puuid/{puuid}", game=game, puuid=puuid));
        self.tick_thresholds().await;

        let resp = client.get(&endpoint)
            .send()
            .await?;

        if resp.status() != StatusCode::OK {
            return Err(SquadOvError::InternalError(format!("Failed to get active shard for game by puuid {} - {}", resp.status().as_u16(), resp.text().await?)));
        }

        #[derive(Deserialize)]
        struct ShardInfo {
            #[serde(rename="activeShard")]
            active_shard: String
        }

        let shard = resp.json::<ShardInfo>().await?;
        Ok(shard.active_shard)
    }
}

impl super::RiotApiApplicationInterface {
    pub async fn obtain_riot_account_from_puuid(&self, puuid: &str) -> Result<(), SquadOvError> {
        let account = self.api.get_account_by_puuid(puuid).await?;
        let mut tx = self.db.begin().await?;
        db::store_riot_account(&mut tx, &account).await?;
        tx.commit().await?;
        Ok(())
    }
    
    pub async fn request_riot_account_from_puuid(&self, puuid: &str) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.queue, serde_json::to_vec(&RiotApiTask::Account{puuid: String::from(puuid)})?, RABBITMQ_DEFAULT_PRIORITY).await;
        Ok(())
    }
}