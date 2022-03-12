use async_trait::async_trait;
use std::sync::Arc;
use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RabbitMqInterface, RabbitMqListener, RabbitMqConfig},
    steam::{
        api::SteamApiClient,
        db,
    },
};
use sqlx::postgres::{PgPool};
use serde::{Serialize, Deserialize};

const STEAM_MAX_AGE_SECONDS: i64 = 86400; // 1 day

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SteamTask {
    ProfileSync {
        steam_ids: Vec<i64>,
    },
}

pub struct SteamApiRabbitmqInterface {
    client: Arc<SteamApiClient>,
    mqconfig: RabbitMqConfig,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
}

impl SteamApiRabbitmqInterface {
    pub fn new (client: Arc<SteamApiClient>, mqconfig: &RabbitMqConfig, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>) -> Self {
        Self {
            client,
            mqconfig: mqconfig.clone(),
            rmq,
            db,
        }
    }

    pub async fn request_sync_steam_accounts(&self, ids: &[i64]) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.steam_queue, serde_json::to_vec(&SteamTask::ProfileSync{
            steam_ids: ids.to_vec(),
        })?, RABBITMQ_DEFAULT_PRIORITY, STEAM_MAX_AGE_SECONDS).await;
        Ok(())
    }
}

#[async_trait]
impl RabbitMqListener for SteamApiRabbitmqInterface {
    async fn handle(&self, data: &[u8], _queue: &str) -> Result<(), SquadOvError> {
        log::info!("Handle Steam RabbitMQ Task: {}", std::str::from_utf8(data).unwrap_or("failure"));
        let task: SteamTask = serde_json::from_slice(data)?;
        match task {
            SteamTask::ProfileSync{steam_ids} => {
                // We want to limit our API calls so we don't keep syncing the same
                // accounts over and over again.
                let steam_ids = db::get_steam_accounts_that_need_sync(&*self.db, &steam_ids).await?;
                if !steam_ids.is_empty() {
                    let summaries = self.client.get_player_summaries(&steam_ids).await?;
                    // Now store all this information in the database as well.
                    let mut tx = self.db.begin().await?;
                    db::sync_steam_player_summaries(&mut tx, &summaries).await?;
                    tx.commit().await?;
                }
            },
        };
        Ok(())
    }
}