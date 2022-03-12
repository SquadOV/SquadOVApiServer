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
    rabbitmq::{RabbitMqInterface, RabbitMqListener, RabbitMqConfig},
    riot::db,
};
use sqlx::postgres::{PgPool};
use reqwest::header;
use tokio::sync::{Semaphore};
use reqwest::{StatusCode, Response};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Deserialize,Debug,Clone)]
pub struct ApiKeyLimit {
    pub requests: usize,
    pub seconds: u64,
    pub enabled: bool
}

#[derive(Deserialize,Debug,Clone)]
pub struct RiotApiKeyConfig {
    pub key: String,
    pub burst_limit: ApiKeyLimit,
    pub bulk_limit: ApiKeyLimit,
}

#[derive(Deserialize,Debug,Clone)]
pub struct RiotConfig {
    pub rso_url: String,
    pub rso_client_id: String,
    pub rso_client_secret: String,
    pub rso_api_key: RiotApiKeyConfig,
    pub valorant_api_key: RiotApiKeyConfig,
    pub lol_api_key: RiotApiKeyConfig,
    pub tft_api_key: RiotApiKeyConfig,
}

pub struct RiotApiHandler {
    api_key: RiotApiKeyConfig,
    burst_threshold: Arc<Semaphore>,
    bulk_threshold: Arc<Semaphore>,
    db: Arc<PgPool>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RiotApiTask {
    UnverifiedAccountLink{
        // Either game_name + tag_line must be set or
        // summoner_name + platform_id
        game_name: Option<String>,
        tag_line: Option<String>,
        summoner_name: Option<String>,
        platform_id: Option<String>,
        user_id: i64,
        raw_puuid: String,
    },
    AccountMe{
        access_token: String,
        refresh_token: String,
        expiration: DateTime<Utc>,
        user_id: i64,
    },
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
        puuid: String,
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
    },
    ValorantMatchCacheData{
        match_uuid: Uuid,
    },
    ValorantMatchPlayerCacheData{
        match_uuid: Uuid,
        user_id: i64,
    }
}

#[derive(Default)]
struct RiotApiStatus {
    down: bool,
    failover: bool,
}

impl RiotApiHandler {
    pub fn new(api_key: RiotApiKeyConfig, db: Arc<PgPool>) -> Self {
        let burst_threshold = Arc::new(Semaphore::new(api_key.burst_limit.requests));
        let bulk_threshold = Arc::new(Semaphore::new(api_key.bulk_limit.requests));

        log::info!("Riot Burst Limit: {} requests/{} seconds: ", api_key.burst_limit.requests, api_key.burst_limit.seconds);
        log::info!("Riot Bulk Limit: {} requests/{} seconds: ", api_key.bulk_limit.requests, api_key.bulk_limit.seconds);

        Self {
            api_key,
            burst_threshold,
            bulk_threshold,
            db,
        }
    }

    // Force a defer if we've manually set a region as being "down".
    pub async fn check_region_status(&self, game: &str, region: &str, allow_failover: bool) -> Result<(), SquadOvError> {
        let game = game.to_lowercase();
        let region = region.to_lowercase();
        let status: RiotApiStatus = sqlx::query_as!(
            RiotApiStatus,
            "
            SELECT down, failover
            FROM squadov.riot_api_outage_status
            WHERE game = $1 AND region = $2
            ",
            &game,
            &region,
        )
            .fetch_optional(&*self.db)
            .await?
            .unwrap_or(RiotApiStatus::default());

        if status.failover && allow_failover {
            Err(SquadOvError::Failover)
        } else if status.down {
            // Delay for an hour
            Err(SquadOvError::Defer(3600000))
        } else {
            Ok(())
        }
    }

    // Ticking the semaphore removes an available request and adds it back *_limit.seconds later.
    // This way we can more accurately ensure that within any *seconds period, we only send
    // *requests. Originally, this was a single thread that looped every *seconds anad refreshed
    // the number of requests to the max amount; this resulted in a problem where we'd go over
    // the rate limit due to the fact that we can use more than the rate limit amount within
    // a given time period (especially if the time period is low).
    async fn tick_burst_threshold(&self) -> Result<(), SquadOvError> {
        if !self.api_key.burst_limit.enabled {
            return Ok(());
        }

        let permit = self.burst_threshold.acquire().await?;
        permit.forget();

        let api_key = self.api_key.clone();
        let threshold = self.burst_threshold.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_secs(api_key.burst_limit.seconds)).await;
            threshold.add_permits(1);
        });
        Ok(())
    }

    async fn tick_bulk_threshold(&self) -> Result<(), SquadOvError> {
        if !self.api_key.bulk_limit.enabled {
            return Ok(());
        }

        let permit = self.bulk_threshold.acquire().await?;
        permit.forget();

        let api_key = self.api_key.clone();
        let threshold = self.bulk_threshold.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_secs(api_key.bulk_limit.seconds)).await;
            threshold.add_permits(1);
        });

        Ok(())
    }

    async fn tick_thresholds(&self) -> Result<(), SquadOvError> {
        self.tick_burst_threshold().await?;
        self.tick_bulk_threshold().await?;
        Ok(())
    }

    fn build_api_endpoint(region: &str, endpoint: &str) -> String {
        format!("https://{}.api.riotgames.com/{}", region, endpoint)
    }

    fn create_http_client(&self) -> Result<reqwest::Client, SquadOvError> {
        let mut headers = header::HeaderMap::new();
        headers.insert("X-Riot-Token", header::HeaderValue::from_str(&self.api_key.key)?);

        Ok(reqwest::ClientBuilder::new()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?)
    }

    async fn check_for_response_error(&self, resp: Response, context: &str) -> Result<Response, SquadOvError> {
        match resp.status() {
            StatusCode::OK => Ok(resp),
            StatusCode::TOO_MANY_REQUESTS => Err(SquadOvError::RateLimit),
            StatusCode::NOT_FOUND => Err(SquadOvError::NotFound),
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE | StatusCode::GATEWAY_TIMEOUT => {
                let url = String::from(resp.url().as_str());
                log::warn!("Failed to query Riot API with a 500-error...retrying: {}", format!(
                    "{context} {status} - {text} [{endpoint}]",
                    context=context,
                    status=resp.status().as_u16(),
                    text=resp.text().await?,
                    endpoint=url,
                ));
                Err(SquadOvError::Defer(1000))
            },
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
    config: RiotConfig,
    api: Arc<RiotApiHandler>,
    mqconfig: RabbitMqConfig,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
}

impl RiotApiApplicationInterface {
    pub fn new (config: RiotConfig, mqconfig: &RabbitMqConfig, api: Arc<RiotApiHandler>, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>) -> Self {
        Self {
            config,
            api,
            mqconfig: mqconfig.clone(),
            rmq,
            db,
        }
    }
}

#[async_trait]
impl RabbitMqListener for RiotApiApplicationInterface {
    async fn handle(&self, data: &[u8], queue: &str) -> Result<(), SquadOvError> {
        log::info!("Handle Riot RabbitMQ Task: {} [{}]", std::str::from_utf8(data).unwrap_or("failure"), queue);
        let task: RiotApiTask = serde_json::from_slice(data)?;
        match task {
            RiotApiTask::UnverifiedAccountLink{game_name, tag_line, summoner_name, platform_id, raw_puuid, user_id} => {
                if game_name.is_some() && tag_line.is_some() {
                    self.perform_unverified_account_link(game_name.unwrap().as_str(), tag_line.unwrap().as_str(), &raw_puuid, user_id).await?;
                } else if summoner_name.is_some() && platform_id.is_some() {
                    self.perform_unverified_summoner_link(summoner_name.unwrap().as_str(), platform_id.unwrap().as_str(), &raw_puuid, user_id).await?;
                } else {
                    log::warn!("...Invalid attempt at performing an unverified account link.");
                    return Err(SquadOvError::BadRequest);
                }
            },
            RiotApiTask::AccountMe{access_token, refresh_token, expiration, user_id} => self.obtain_riot_account_from_access_token(&access_token, &refresh_token, &expiration, user_id).await.and(Ok(()))?,
            RiotApiTask::Account{puuid} => self.obtain_riot_account_from_puuid(&puuid).await?,
            RiotApiTask::ValorantBackfill{puuid} => self.backfill_user_valorant_matches(&puuid).await?,
            RiotApiTask::ValorantMatch{match_id, shard} => match self.obtain_valorant_match_info(&match_id, &shard, queue == self.mqconfig.valorant_queue).await {
                Ok(_) => (),
                Err(err) => match err {
                    SquadOvError::Failover => return Err(SquadOvError::SwitchQueue(self.mqconfig.failover_valorant_queue.clone())),
                    // Remap not found to defer because Rito's api might not be that fast to give us the info right as the game finishes.
                    SquadOvError::NotFound => return Err(SquadOvError::Defer(60 * 1000)),
                    _ => return Err(err)
                }
            },
            RiotApiTask::LolBackfill{puuid, platform} => self.backfill_user_lol_matches(&puuid, &platform).await?,
            RiotApiTask::LolMatch{platform, game_id} => match self.obtain_lol_match_info(&platform, game_id).await {
                Ok(_) => (),
                Err(err) => match err {
                    // Remap not found to defer because Rito's api might not be that fast to give us the info right as the game finishes.
                    SquadOvError::NotFound => return Err(SquadOvError::Defer(60 * 1000)),
                    _ => return Err(err)
                }
            },
            RiotApiTask::TftBackfill{puuid, region} => self.backfill_user_tft_matches(&puuid, &region).await?,
            RiotApiTask::TftMatch{platform, region, game_id} => match self.obtain_tft_match_info(&platform, &region, game_id).await {
                Ok(_) => (),
                Err(err) => match err {
                    // Remap not found to defer because chances are the game hasn't finished yet so we need to wait a bit before trying again.
                    SquadOvError::NotFound => return Err(SquadOvError::Defer(60 * 1000)),
                    _ => return Err(err)
                }
            },
            RiotApiTask::ValorantMatchCacheData{match_uuid} => {
                let mut tx = self.db.begin().await?;
                db::cache_valorant_match_information(&mut tx, &match_uuid).await?;
                let match_user_ids = db::get_squadov_user_ids_in_valorant_match(&mut tx, &match_uuid).await?;
                for user_id in match_user_ids {
                    db::cache_valorant_player_pov_information(&mut tx, &match_uuid, user_id).await?;
                }
                tx.commit().await?;
            },
            RiotApiTask::ValorantMatchPlayerCacheData{match_uuid, user_id} => {
                let mut tx = self.db.begin().await?;
                db::cache_valorant_player_pov_information(&mut tx, &match_uuid, user_id).await?;
                tx.commit().await?;
            },
        };
        Ok(())
    }
}

pub fn riot_region_to_routing(region: &str) -> Result<String, SquadOvError> {
    let region = region.to_uppercase();

    Ok(String::from(
        if region.starts_with("NA") || region.starts_with("BR") || region.starts_with("LAN") || region.starts_with("LAS") || region.starts_with("OC") {
            "americas"
        } else if region.starts_with("KR") || region.starts_with("JP") {
            "asia"
        } else if region.starts_with("EU") || region.starts_with("TR") || region.starts_with("RU") {
            "europe"
        } else {
            return Err(SquadOvError::BadRequest);
        }
    ))
}