use async_trait::async_trait;

mod account;
mod valorant;
mod lol;
mod tft;
mod summoner;

use serde::{Serialize, Deserialize};
use std::sync::Arc;
use crate::{
    SquadOvError,
    rabbitmq::{RabbitMqInterface, RabbitMqListener}
};
use sqlx::postgres::{PgPool};
use reqwest::header;
use tokio::sync::{Semaphore};
use reqwest::{StatusCode, Response};

#[derive(Deserialize,Debug,Clone)]
pub struct ApiKeyLimit {
    pub requests: usize,
    pub seconds: u64,
}

#[derive(Deserialize,Debug,Clone)]
pub struct RiotApiKeyConfig {
    pub key: String,
    pub burst_limit: ApiKeyLimit,
    pub bulk_limit: ApiKeyLimit,
}

#[derive(Deserialize,Debug,Clone)]
pub struct RiotConfig {
    pub valorant_api_key: RiotApiKeyConfig,
    pub lol_api_key: RiotApiKeyConfig,
    pub tft_api_key: RiotApiKeyConfig,
}

pub struct RiotApiHandler {
    api_key: RiotApiKeyConfig,
    burst_threshold: Arc<Semaphore>,
    bulk_threshold: Arc<Semaphore>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RiotApiTask {
    Account{
        puuid: String
    },
    TftBackfill{
        puuid: String,
        region: String,
    },
    TftMatch{
        platform: String,
        region: String,
        game_id: i64,
    },
    LolBackfill{
        account_id: String,
        platform: String,
    },
    LolMatch{
        platform: String,
        game_id: i64,
    },
    ValorantBackfill{
        puuid: String
    },
    ValorantMatch{
        match_id: String,
        shard: String,
    }
}

impl RiotApiHandler {
    pub fn new(api_key: RiotApiKeyConfig) -> Self {
        let burst_threshold = Arc::new(Semaphore::new(api_key.burst_limit.requests));
        let bulk_threshold = Arc::new(Semaphore::new(api_key.bulk_limit.requests));

        log::info!("Riot Burst Limit: {} requests/{} seconds: ", api_key.burst_limit.requests, api_key.burst_limit.seconds);
        log::info!("Riot Bulk Limit: {} requests/{} seconds: ", api_key.bulk_limit.requests, api_key.bulk_limit.seconds);

        Self {
            api_key,
            burst_threshold,
            bulk_threshold,
        }
    }

    // Ticking the semaphore removes an available request and adds it back *_limit.seconds later.
    // This way we can more accurately ensure that within any *seconds period, we only send
    // *requests. Originally, this was a single thread that looped every *seconds anad refreshed
    // the number of requests to the max amount; this resulted in a problem where we'd go over
    // the rate limit due to the fact that we can use more than the rate limit amount within
    // a given time period (especially if the time period is low).
    async fn tick_burst_threshold(&self) {
        let permit = self.burst_threshold.acquire().await;
        permit.forget();

        let api_key = self.api_key.clone();
        let threshold = self.burst_threshold.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_secs(api_key.burst_limit.seconds)).await;
            threshold.add_permits(1);
        });
    }

    async fn tick_bulk_threshold(&self) {
        let permit = self.bulk_threshold.acquire().await;
        permit.forget();

        let api_key = self.api_key.clone();
        let threshold = self.bulk_threshold.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_secs(api_key.bulk_limit.seconds)).await;
            threshold.add_permits(1);
        });
    }

    async fn tick_thresholds(&self) {
        self.tick_burst_threshold().await;
        self.tick_bulk_threshold().await;
    }

    fn build_api_endpoint(region: &str, endpoint: &str) -> String {
        format!("https://{}.api.riotgames.com/{}", region, endpoint)
    }

    fn create_http_client(&self) -> Result<reqwest::Client, SquadOvError> {
        let mut headers = header::HeaderMap::new();
        headers.insert("X-Riot-Token", header::HeaderValue::from_str(&self.api_key.key)?);

        Ok(reqwest::ClientBuilder::new()
            .default_headers(headers)
            .build()?)
    }

    async fn check_for_response_error(&self, resp: Response, context: &str) -> Result<Response, SquadOvError> {
        match resp.status() {
            StatusCode::OK => Ok(resp),
            StatusCode::TOO_MANY_REQUESTS => Err(SquadOvError::RateLimit),
            StatusCode::NOT_FOUND => Err(SquadOvError::NotFound),
            _ => {
                let url = String::from(resp.url().as_str());
                Err(SquadOvError::InternalError(format!(
                    "{context} {status} - {text} [{endpoint}]",
                    context=context,
                    status=resp.status().as_u16(),
                    text=resp.text().await?,
                    endpoint=url,
                )))
            }
        }
    }
}

pub struct RiotApiApplicationInterface {
    api: Arc<RiotApiHandler>,
    queue: String,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
}

impl RiotApiApplicationInterface {
    pub fn new (queue: &str, api: Arc<RiotApiHandler>, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>) -> Self {
        Self {
            api,
            queue: String::from(queue),
            rmq,
            db,
        }
    }
}

#[async_trait]
impl RabbitMqListener for RiotApiApplicationInterface {
    async fn handle(&self, data: &[u8]) -> Result<(), SquadOvError> {
        let task: RiotApiTask = serde_json::from_slice(data)?;
        match task {
            RiotApiTask::Account{puuid} => self.obtain_riot_account_from_puuid(&puuid).await?,
            RiotApiTask::ValorantBackfill{puuid} => self.backfill_user_valorant_matches(&puuid).await?,
            RiotApiTask::ValorantMatch{match_id, shard} => self.obtain_valorant_match_info(&match_id, &shard).await?,
            RiotApiTask::LolBackfill{account_id, platform} => self.backfill_user_lol_matches(&account_id, &platform).await?,
            RiotApiTask::LolMatch{platform, game_id} => self.obtain_lol_match_info(&platform, game_id).await?,
            RiotApiTask::TftBackfill{puuid, region} => self.backfill_user_tft_matches(&puuid, &region).await?,
            RiotApiTask::TftMatch{platform, region, game_id} => match self.obtain_tft_match_info(&platform, &region, game_id).await {
                Ok(_) => (),
                Err(err) => match err {
                    // Remap not found to defer because chances are the game hasn't finished yet so we need to wait a bit before trying again.
                    SquadOvError::NotFound => return Err(SquadOvError::Defer(60 * 1000)),
                    _ => return Err(err)
                }
            },
        };
        Ok(())
    }
}

pub fn riot_region_to_routing(region: &str) -> Result<String, SquadOvError> {
    let region = region.to_uppercase();

    Ok(String::from(match region.as_str() {
        "NA" | "BR" | "LAN" | "LAS" | "OCE" => "americas",
        "KR" | "JP" => "asia",
        "EUNE" | "EUW" | "TR" | "RU" => "europe",
        _ => return Err(SquadOvError::BadRequest)
    }))
}