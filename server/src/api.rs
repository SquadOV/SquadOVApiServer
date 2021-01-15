pub mod api_service;
pub mod auth;
pub mod fusionauth;
pub mod access;
pub mod v1;
pub mod graphql;
pub mod admin;

use serde::{Deserialize};
use sqlx::postgres::{PgPool};
use actix_web::{HttpRequest};
use squadov_common;
use squadov_common::{
    SquadOvError,
    HalResponse,
    BlobManagementClient,
    JobQueue,
    KafkaCredentialKeyPair,
    riot::{
        api::{RiotApiHandler, RiotApiApplicationInterface, RiotConfig},
    },
    rabbitmq::{RabbitMqInterface, RabbitMqConfig}
};
use url::Url;
use std::vec::Vec;
use std::sync::Arc;
use sqlx::postgres::{PgPoolOptions};

// TODO: REMOVE THIS.
#[macro_export]
macro_rules! logged_error {
    ($x:expr) => {{
        warn!("{}", $x); Err($x)
    }};
}


#[derive(Deserialize)]
pub struct PaginationParameters {
    pub start: i64,
    pub end: i64
}

fn replace_pagination_parameters_in_url(url: &str, start : i64, end : i64) -> Result<String, SquadOvError> {
    let mut url = Url::parse(url)?;
    let mut query_params: Vec<(String, String)> = url.query_pairs().into_owned().collect();

    {
        let mut new_params = url.query_pairs_mut();
        new_params.clear();

        for pair in &mut query_params {
            if pair.0 == "start" {
                pair.1 = format!("{}", start);
            } else if pair.0 == "end" {
                pair.1 = format!("{}", end);
            }
            new_params.append_pair(&pair.0, &pair.1);
        }
    }

    Ok(String::from(url.as_str()))
}

pub fn construct_hal_pagination_response<T>(data : T, req: &HttpRequest, params: &PaginationParameters, has_next: bool) -> Result<HalResponse<T>, SquadOvError> {
    let conn = req.connection_info();
    let raw_url = format!("{}://{}{}", conn.scheme(), conn.host(), req.uri().to_string());
    let count = params.end - params.start;

    let mut response = HalResponse::new(data);
    response.add_link("self", &raw_url);

    if has_next {
        let next_start = params.end;
        let next_end = params.end + count;
        response.add_link("next", &replace_pagination_parameters_in_url(&raw_url, next_start, next_end)?);
    }

    if params.start != 0 {
        let prev_start = params.start - count;
        let prev_end = params.start;
        response.add_link("prev", &replace_pagination_parameters_in_url(&raw_url, prev_start, prev_end)?);
    }

    Ok(response)
}

#[derive(Deserialize,Debug,Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub connections: u32
}

#[derive(Deserialize,Debug,Clone)]
pub struct CorsConfig {
    pub domain: String
}

#[derive(Deserialize,Debug,Clone)]
pub struct ServerConfig {
    pub domain: String,
    pub graphql_debug: bool
}

#[derive(Deserialize,Debug,Clone)]
pub struct GitlabConfig {
    pub access_token: String,
    pub project_id: u64
}

#[derive(Deserialize,Debug,Clone)]
pub struct VodConfig {
    pub fastify_threads: i32
}

#[derive(Deserialize,Debug,Clone)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub wow_combat_log_threads: i32,
    pub client_keypair: KafkaCredentialKeyPair,
    pub server_keypair: KafkaCredentialKeyPair
}

#[derive(Deserialize,Debug,Clone)]
pub struct ApiConfig {
    fusionauth: fusionauth::FusionAuthConfig,
    pub gcp: squadov_common::GCPConfig,
    pub database: DatabaseConfig,
    pub cors: CorsConfig,
    pub server: ServerConfig,
    pub gitlab: GitlabConfig,
    pub kafka: KafkaConfig,
    pub vod: VodConfig,
    pub riot: RiotConfig,
    pub rabbitmq: RabbitMqConfig,
}

struct ApiClients {
    fusionauth: fusionauth::FusionAuthClient,
}

pub struct ApiApplication {
    pub config: ApiConfig,
    clients: ApiClients,
    users: auth::UserManager,
    session: auth::SessionManager,
    vod: Arc<dyn v1::VodManager + Send + Sync>,
    pub pool: Arc<PgPool>,
    schema: Arc<graphql::GraphqlSchema>,
    pub blob: Arc<BlobManagementClient>,
    // Various local job queues - these should eventually
    // probably be switched to something like RabbitMQ + microservices.
    pub vod_fastify_jobs: Arc<JobQueue<v1::VodFastifyJob>>,
    pub valorant_itf: Arc<RiotApiApplicationInterface>,
    pub lol_itf: Arc<RiotApiApplicationInterface>,
    pub tft_itf: Arc<RiotApiApplicationInterface>,
}

impl ApiApplication {
    pub async fn new(config: &ApiConfig) -> ApiApplication {
        // Use TOML config to create application - e.g. for
        // database configuration, external API client configuration, etc.
        let pool = Arc::new(PgPoolOptions::new()
            .max_connections(config.database.connections)
            .connect(&config.database.url)
            .await
            .unwrap());

        let gcp = Arc::new(
            if config.gcp.enabled {
                Some(squadov_common::GCPClient::new(&config.gcp).await)
            } else {
                None
            }
        );

        let blob = Arc::new(BlobManagementClient::new(gcp.clone(), pool.clone()));
        
        let valorant_api = Arc::new(RiotApiHandler::new(config.riot.valorant_api_key.clone()));
        let lol_api = Arc::new(RiotApiHandler::new(config.riot.lol_api_key.clone()));
        let tft_api = Arc::new(RiotApiHandler::new(config.riot.tft_api_key.clone()));
        let rabbitmq = Arc::new(RabbitMqInterface::new(&config.rabbitmq).await.unwrap());

        let valorant_itf = Arc::new(RiotApiApplicationInterface::new(&config.rabbitmq.valorant_queue, valorant_api.clone(), rabbitmq.clone(), pool.clone()));
        let lol_itf = Arc::new(RiotApiApplicationInterface::new(&config.rabbitmq.lol_queue, lol_api.clone(), rabbitmq.clone(), pool.clone()));
        let tft_itf = Arc::new(RiotApiApplicationInterface::new(&config.rabbitmq.tft_queue, tft_api.clone(), rabbitmq.clone(), pool.clone()));

        rabbitmq.add_listener(config.rabbitmq.valorant_queue.clone(), valorant_itf.clone()).await;
        rabbitmq.add_listener(config.rabbitmq.lol_queue.clone(), lol_itf.clone()).await;
        rabbitmq.add_listener(config.rabbitmq.tft_queue.clone(), tft_itf.clone()).await;

        ApiApplication{
            config: config.clone(),
            clients: ApiClients{
                fusionauth: fusionauth::FusionAuthClient::new(config.fusionauth.clone()),
            },
            users: auth::UserManager{},
            session: auth::SessionManager::new(),
            vod: match v1::get_current_vod_manager_type() {
                v1::VodManagerType::GCS => Arc::new(v1::GCSVodManager::new(gcp.clone()).await.unwrap()) as Arc<dyn v1::VodManager + Send + Sync>,
                v1::VodManagerType::FileSystem => Arc::new(v1::FilesystemVodManager::new().unwrap()) as Arc<dyn v1::VodManager + Send + Sync>
            },
            pool: pool,
            schema: Arc::new(graphql::create_schema()),
            blob: blob,
            vod_fastify_jobs: JobQueue::new::<v1::VodFastifyWorker>(config.vod.fastify_threads),
            valorant_itf,
            lol_itf,
            tft_itf,
        }
    }
}