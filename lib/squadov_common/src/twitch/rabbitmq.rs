use async_trait::async_trait;
use std::sync::Arc;
use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RabbitMqInterface, RabbitMqListener, RabbitMqConfig},
    twitch::{
        api::{
            TwitchApiClient,
            TwitchTokenType,
        },
        TwitchConfig,
        eventsub,
    },
    accounts::twitch,
};
use sqlx::postgres::{PgPool};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TwitchTask {
    SubscriberSync {
        twitch_user_id: String,
        cursor: Option<String>,
    },
}

pub struct TwitchApiRabbitmqInterface {
    tvconfig: TwitchConfig,
    mqconfig: RabbitMqConfig,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
}

const TWITCH_MAX_AGE_SECONDS: i64 = 86400; // 1 day

impl TwitchApiRabbitmqInterface {
    pub fn new (tvconfig: TwitchConfig, mqconfig: RabbitMqConfig, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>) -> Self {
        Self {
            tvconfig,
            mqconfig,
            rmq,
            db,
        }
    }

    pub async fn request_sync_subscriber(&self, twitch_id: &str, cursor: Option<String>) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.twitch_queue, serde_json::to_vec(&TwitchTask::SubscriberSync{
            twitch_user_id: String::from(twitch_id),
            cursor,
        })?, RABBITMQ_DEFAULT_PRIORITY, TWITCH_MAX_AGE_SECONDS).await;
        Ok(())
    }
}

#[async_trait]
impl RabbitMqListener for TwitchApiRabbitmqInterface {
    async fn handle(&self, data: &[u8], _queue: &str) -> Result<(), SquadOvError> {
        log::info!("Handle Twitch API Task: {}", std::str::from_utf8(data).unwrap_or("failure"));
        let task: TwitchTask = serde_json::from_slice(data)?;
        match task {
            TwitchTask::SubscriberSync{twitch_user_id, cursor} => {
                let token = twitch::get_twitch_oauth_token(&*self.db, &twitch_user_id).await?;
                let (subs, cursor) = {
                    let client = TwitchApiClient::new(self.tvconfig.clone(), token, TwitchTokenType::User, self.db.clone());
                    client.get_broadcaster_subscriptions(&twitch_user_id, cursor).await?
                };

                // Store subs in database.
                eventsub::store_twitch_subs(&*self.db, &subs).await?;

                // If cursor is not none, put in a new task to get more!
                if let Some(cursor) = cursor {
                    self.request_sync_subscriber(&twitch_user_id, Some(cursor)).await?;
                }
            },
        };
        Ok(())
    }
}