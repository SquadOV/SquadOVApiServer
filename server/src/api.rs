pub mod api_service;
pub mod auth;
pub mod fusionauth;
pub mod access;
pub mod v1;
pub mod graphql;
pub mod admin;
pub mod oembed;
pub mod meta;

use serde::{Deserialize};
use sqlx::postgres::{PgPool};
use actix_web::{HttpRequest};
use squadov_common;
use squadov_common::{
    SquadOvError,
    HalResponse,
    KafkaCredentialKeyPair,
    riot::{
        api::{RiotApiHandler, RiotApiApplicationInterface, RiotConfig},
    },
    rabbitmq::{RabbitMqInterface, RabbitMqConfig},
    EmailConfig,
    EmailClient,
    vod,
    vod::VodProcessingInterface,
    vod::manager::{
        VodManagerType,
        VodManager,
        GCSVodManager,
        FilesystemVodManager,
        S3VodManager,
    },
    csgo::rabbitmq::CsgoRabbitmqInterface,
    steam::{
        api::{SteamApiConfig, SteamApiClient},
        rabbitmq::SteamApiRabbitmqInterface,
    },
    blob,
    blob::{
        BlobManagerType,
        BlobStorageClient,
        BlobManagementClient,
        gcp::GCPBlobStorage,
        aws::AWSBlobStorage,
    },
    storage::{StorageManager, CloudStorageLocation, CloudStorageBucketsConfig},
    GCPClient,
    aws::{
        AWSClient,
        AWSConfig,
    },
    ipstack::{
        IpstackConfig,
        IpstackClient,
    },
    segment::{
        SegmentConfig,
        SegmentClient,
    },
    user::SquadOVUser,
    twitch::{
        TwitchConfig,
        rabbitmq::TwitchApiRabbitmqInterface,
        api::{
            TwitchApiClient,
            TwitchTokenType,
        },
        oauth,
    },
    config::CommonConfig,
};
use url::Url;
use std::vec::Vec;
use std::sync::{Arc};
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

    // Sometimes the query key has [] in it for array parameters. The URL
    // will url encode those characters which causes them to be invalid so
    // we need to keep them in their original form.
    Ok(String::from(url.as_str()).replace(
        "%5B", "[",
    ).replace(
        "%5D", "]",
    ))
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
    pub connections: u32,
    pub heavy_connections: u32,
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
    pub wow_combat_log_topic: String,
    pub client_keypair: KafkaCredentialKeyPair,
    pub server_keypair: KafkaCredentialKeyPair
}

#[derive(Deserialize,Debug,Clone)]
pub struct SentryConfig {
    pub client_service_dsn: String,
    pub web_dsn: String,
}

#[derive(Deserialize,Debug,Clone)]
pub struct SquadOvConfig {
    pub app_url: String,
    pub landing_url: String,
    pub invite_key: String,
    pub share_key: String,
    pub access_key: String,
    pub hashid_salt: String,
}

#[derive(Deserialize,Debug,Clone)]
pub struct SquadOvStorageConfig {
    pub vods: CloudStorageBucketsConfig,
    pub blobs: CloudStorageBucketsConfig,
}

#[derive(Deserialize,Debug,Clone)]
pub struct ApiConfig {
    fusionauth: fusionauth::FusionAuthConfig,
    pub gcp: squadov_common::GCPConfig,
    pub aws: AWSConfig,
    pub database: DatabaseConfig,
    pub cors: CorsConfig,
    pub server: ServerConfig,
    pub gitlab: GitlabConfig,
    pub kafka: KafkaConfig,
    pub vod: VodConfig,
    pub riot: RiotConfig,
    pub rabbitmq: RabbitMqConfig,
    pub email: EmailConfig,
    pub squadov: SquadOvConfig,
    pub steam: SteamApiConfig,
    pub twitch: TwitchConfig,
    pub storage: SquadOvStorageConfig,
    pub ipstack: IpstackConfig,
    pub segment: SegmentConfig,
    pub sentry: SentryConfig,
}

impl CommonConfig for DatabaseConfig {
    fn read_from_env(&mut self) {
        if let Ok(connections) = std::env::var("SQUADOV_DB_CONNECTIONS") {
            self.connections = connections.parse::<u32>().unwrap_or(self.connections);
        }

        if let Ok(connections) = std::env::var("SQUADOV_DB_HEAVY_CONNECTIONS") {
            self.heavy_connections = connections.parse::<u32>().unwrap_or(self.heavy_connections);
        }
    }
}

impl CommonConfig for ApiConfig {
    fn read_from_env(&mut self) {
        self.fusionauth.read_from_env();
        self.database.read_from_env();
    }
}

pub struct ApiClients {
    pub fusionauth: fusionauth::FusionAuthClient,
}

pub struct ApiApplication {
    pub config: ApiConfig,
    pub clients: ApiClients,
    pub users: auth::UserManager,
    session: auth::SessionManager,
    vod: Arc<StorageManager<Arc<dyn VodManager + Send + Sync>>>,
    pub pool: Arc<PgPool>,
    pub heavy_pool: Arc<PgPool>,
    schema: Arc<graphql::GraphqlSchema>,
    pub blob: Arc<StorageManager<Arc<BlobManagementClient>>>,
    pub rso_itf: Arc<RiotApiApplicationInterface>,
    pub valorant_itf: Arc<RiotApiApplicationInterface>,
    pub lol_itf: Arc<RiotApiApplicationInterface>,
    pub tft_itf: Arc<RiotApiApplicationInterface>,
    pub email: Arc<EmailClient>,
    pub vod_itf: Arc<VodProcessingInterface>,
    pub csgo_itf: Arc<CsgoRabbitmqInterface>,
    pub steam_itf: Arc<SteamApiRabbitmqInterface>,
    pub twitch_itf: Arc<TwitchApiRabbitmqInterface>,
    gcp: Arc<Option<GCPClient>>,
    aws: Arc<Option<AWSClient>>,
    pub hashid: Arc<harsh::Harsh>,
    pub ip: Arc<IpstackClient>,
    pub segment: Arc<SegmentClient>,
    pub twitch_api: Arc<TwitchApiClient>,
}

impl ApiApplication {
    async fn mark_users_inactive(&self) -> Result<(), SquadOvError> {
        let inactive_users = sqlx::query_as!(
            SquadOVUser,
            "
            SELECT DISTINCT
                u.id,
                u.username,
                u.email,
                u.verified,
                u.uuid,
                u.is_test,
                u.is_admin,
                u.welcome_sent,
                u.registration_time
            FROM squadov.users AS u
            LEFT JOIN squadov.daily_active_sessions AS das
                ON das.user_id = u.id
                    AND das.tm >= (NOW() - INTERVAL '14 day')
            LEFT JOIN squadov.user_event_record AS uer
                ON uer.user_id = u.id
                    AND uer.event_name = 'inactive_14'
            WHERE uer.user_id IS NULL
                AND das.user_id IS NULL
            ",
        )
            .fetch_all(&*self.pool)
            .await?;

        // Mark these users as being inactive via Segment. TODO this should probably be done in bulk
        for u in &inactive_users {
            // Do one more identify just in case the user was active before we started doing these identifies so Vero
            // doesn't have the info on them.
            self.analytics_identify_user(u, "", "").await?;
            self.segment.track(&u.uuid.to_string(), "inactive_14").await?;
        }

        // Then mark them as being inactive in the database.
        self.record_user_event(&inactive_users.iter().map(|x| {
            x.id
        }).collect::<Vec<i64>>(), "inactive_14").await?;
        
        Ok(())
    }

    pub async fn get_vod_manager(&self, bucket: &str) -> Result<Arc<dyn VodManager + Send + Sync>, SquadOvError> {
        self.vod.get_bucket(bucket).await.ok_or(SquadOvError::NotFound)
    }

    async fn create_vod_manager(&mut self, bucket: &str) -> Result<(), SquadOvError> {
        let vod_manager = match vod::manager::get_vod_manager_type(bucket) {
            VodManagerType::GCS => Arc::new(GCSVodManager::new(bucket, self.gcp.clone()).await?) as Arc<dyn VodManager + Send + Sync>,
            VodManagerType::S3 => Arc::new(S3VodManager::new(bucket, self.aws.clone(), self.config.aws.cdn.clone()).await?) as Arc<dyn VodManager + Send + Sync>,
            VodManagerType::FileSystem => Arc::new(FilesystemVodManager::new(bucket)?) as Arc<dyn VodManager + Send + Sync>
        };
        self.vod.new_bucket(bucket, vod_manager).await;
        Ok(())
    }

    pub async fn get_blob_manager(&self, bucket: &str) -> Result<Arc<BlobManagementClient>, SquadOvError> {
        self.blob.get_bucket(bucket).await.ok_or(SquadOvError::NotFound)
    }

    async fn create_blob_manager(&mut self, bucket: &str) -> Result<(), SquadOvError> {
        let storage = match blob::get_blob_manager_type(bucket) {
            BlobManagerType::GCS => Arc::new(GCPBlobStorage::new(self.gcp.clone())) as Arc<dyn BlobStorageClient + Send + Sync>,
            BlobManagerType::S3 => Arc::new(AWSBlobStorage::new(self.aws.clone(), self.config.aws.cdn.clone())) as Arc<dyn BlobStorageClient + Send + Sync>,
        };
        self.blob.new_bucket(bucket, Arc::new(BlobManagementClient::new(bucket, self.pool.clone(), storage))).await;
        Ok(())
    }

    pub async fn new(config: &ApiConfig) -> ApiApplication {
        let disable_rabbitmq = std::env::var("DISABLE_RABBITMQ").is_ok();

        // Use TOML config to create application - e.g. for
        // database configuration, external API client configuration, etc.
        let pool = Arc::new(PgPoolOptions::new()
            .min_connections(1)
            .max_connections(config.database.connections)
            .max_lifetime(std::time::Duration::from_secs(6*60*60))
            .idle_timeout(std::time::Duration::from_secs(3*60*60))
            .connect(&config.database.url)
            .await
            .unwrap());

        let heavy_pool = Arc::new(PgPoolOptions::new()
            .min_connections(1)
            .max_connections(config.database.heavy_connections)
            .max_lifetime(std::time::Duration::from_secs(3*60*60))
            .idle_timeout(std::time::Duration::from_secs(1*60*60))
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

        let aws = Arc::new(
            if config.aws.enabled {
                Some(AWSClient::new(&config.aws))
            } else {
                None
            }
        );

        let rso_api = Arc::new(RiotApiHandler::new(config.riot.rso_api_key.clone()));
        let valorant_api = Arc::new(RiotApiHandler::new(config.riot.valorant_api_key.clone()));
        let lol_api = Arc::new(RiotApiHandler::new(config.riot.lol_api_key.clone()));
        let tft_api = Arc::new(RiotApiHandler::new(config.riot.tft_api_key.clone()));
        let steam_api = Arc::new(SteamApiClient::new(&config.steam));

        let rabbitmq = RabbitMqInterface::new(&config.rabbitmq, pool.clone(), !disable_rabbitmq).await.unwrap();

        let rso_itf = Arc::new(RiotApiApplicationInterface::new(config.riot.clone(), &config.rabbitmq, rso_api.clone(), rabbitmq.clone(), pool.clone()));
        let valorant_itf = Arc::new(RiotApiApplicationInterface::new(config.riot.clone(), &config.rabbitmq, valorant_api.clone(), rabbitmq.clone(), pool.clone()));
        let lol_itf = Arc::new(RiotApiApplicationInterface::new(config.riot.clone(), &config.rabbitmq, lol_api.clone(), rabbitmq.clone(), pool.clone()));
        let tft_itf = Arc::new(RiotApiApplicationInterface::new(config.riot.clone(), &config.rabbitmq, tft_api.clone(), rabbitmq.clone(), pool.clone()));
        let steam_itf = Arc::new(SteamApiRabbitmqInterface::new(steam_api.clone(), &config.rabbitmq, rabbitmq.clone(), pool.clone()));
        let csgo_itf = Arc::new(CsgoRabbitmqInterface::new(steam_itf.clone(), &config.rabbitmq, rabbitmq.clone(), pool.clone()));
        let twitch_itf = Arc::new(TwitchApiRabbitmqInterface::new(config.twitch.clone(), config.rabbitmq.clone(), rabbitmq.clone(), pool.clone()));

        if !disable_rabbitmq {
            if config.rabbitmq.enable_rso {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.rso_queue.clone(), rso_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_valorant {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.valorant_queue.clone(), valorant_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_lol {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.lol_queue.clone(), lol_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_tft {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.tft_queue.clone(), tft_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_steam {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.steam_queue.clone(), steam_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_csgo {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.csgo_queue.clone(), csgo_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }

            if config.rabbitmq.enable_twitch {
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.twitch_queue.clone(), twitch_itf.clone(), config.rabbitmq.prefetch_count).await.unwrap();
            }
        }

        let mut vod_manager = StorageManager::<Arc<dyn VodManager + Send + Sync>>::new(); 
        vod_manager.set_location_map(CloudStorageLocation::Global, &config.storage.vods.global);
        let vod_manager = Arc::new(vod_manager);

        let mut blob = StorageManager::<Arc<BlobManagementClient>>::new();
        blob.set_location_map(CloudStorageLocation::Global, &config.storage.blobs.global);
        let blob = Arc::new(blob);

        // One VOD interface for publishing - individual interfaces for consuming.
        let vod_itf = Arc::new(VodProcessingInterface::new(&config.rabbitmq.vod_queue, rabbitmq.clone(), pool.clone(), vod_manager.clone()));
        if !disable_rabbitmq && config.rabbitmq.enable_vod {
            for _i in 0..config.vod.fastify_threads {
                let process_itf = Arc::new(VodProcessingInterface::new(&config.rabbitmq.vod_queue, rabbitmq.clone(), pool.clone(), vod_manager.clone()));
                RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.vod_queue.clone(), process_itf, config.rabbitmq.prefetch_count).await.unwrap();
            }
        }

        let mut app = ApiApplication{
            config: config.clone(),
            clients: ApiClients{
                fusionauth: fusionauth::FusionAuthClient::new(config.fusionauth.clone()),
            },
            users: auth::UserManager{},
            session: auth::SessionManager::new(),
            vod: vod_manager,
            pool: pool.clone(),
            heavy_pool,
            schema: Arc::new(graphql::create_schema()),
            blob: blob,
            rso_itf,
            valorant_itf,
            lol_itf,
            tft_itf,
            email: Arc::new(EmailClient::new(&config.email)),
            vod_itf,
            csgo_itf,
            steam_itf,
            twitch_itf,
            gcp,
            aws,
            hashid: Arc::new(harsh::Harsh::builder().salt(config.squadov.hashid_salt.as_str()).length(6).build().unwrap()),
            ip: Arc::new(IpstackClient::new(config.ipstack.clone())),
            segment: Arc::new(SegmentClient::new(config.segment.clone())),
            twitch_api: Arc::new(TwitchApiClient::new(
                config.twitch.clone(),
                oauth::get_oauth_client_credentials_token(&config.twitch.client_id, &config.twitch.client_secret).await.unwrap(),
                TwitchTokenType::App,
                pool.clone(),
            )),
        };

        app.create_vod_manager(&config.storage.vods.global).await.unwrap();
        if config.storage.vods.global != config.storage.vods.legacy {
            app.create_vod_manager(&config.storage.vods.legacy).await.unwrap();
        }

        app.create_blob_manager(&config.storage.blobs.global).await.unwrap();
        if config.storage.blobs.global != config.storage.blobs.legacy {
            app.create_blob_manager(&config.storage.blobs.legacy).await.unwrap();
        }

        app
    }
}

pub fn start_event_loop(app: Arc<ApiApplication>) {
    tokio::task::spawn(async move {
        loop {
            log::info!("Ticking Event Loop...");

            log::info!("...Checking for inactive users.");
            match app.mark_users_inactive().await {
                Ok(()) => (),
                Err(err) => log::warn!("...Failed to mark inactive users: {:?}", err),
            };

            log::info!("...Reverifying Twitch accounts.");
            match app.reverify_twitch_account_access_tokens().await {
                Ok(()) => (),
                Err(err) => log::warn!("...Failed to reverify twitch accounts: {:?}", err),
            };

            // Doing this once per day should be sufficient...
            tokio::time::delay_for(tokio::time::Duration::from_secs(86400)).await;
        }
    });
}