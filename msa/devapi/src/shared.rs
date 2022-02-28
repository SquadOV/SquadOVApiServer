use serde::Deserialize;
use sqlx::{
    ConnectOptions,
    PgPool,
    postgres::{PgPoolOptions, PgConnectOptions},
};
use std::sync::Arc;

#[derive(Deserialize, Debug, Clone)]
pub struct DevApiConfig {
    db_endpoint: String,
    db_username: String,
    db_password: String,
    db_connections: u32,
    pub fa_url: String,
    pub fa_tenant_id: String,
    pub fa_client_id: String,
    pub fa_client_secret: String,
    pub self_host: String,
    pub self_schema: String,
}

impl DevApiConfig {
    pub fn self_url(&self) -> String {
        format!("{}://{}", &self.self_schema, &self.self_host)
    }

    pub fn secure(&self) -> bool {
        self.self_host == "https"
    }
}


pub struct SharedApp {
    pub config: DevApiConfig,
    pub pool: Arc<PgPool>,
}

impl SharedApp {
    pub async fn new(config: DevApiConfig) -> Self {
        Self {
            config: config.clone(),
            pool: {
                let mut conn = PgConnectOptions::new()
                    .host(&config.db_endpoint)
                    .username(&config.db_username)
                    .password(&config.db_password)
                    .port(5432)
                    .application_name("squadov_devapi")
                    .database("squadov")
                    .statement_cache_capacity(0);
                conn.log_statements(log::LevelFilter::Trace);
                Arc::new(PgPoolOptions::new()
                    .min_connections(1)
                    .max_connections(config.db_connections)
                    .max_lifetime(std::time::Duration::from_secs(6*60*60))
                    .idle_timeout(std::time::Duration::from_secs(3*60*60))
                    .connect_with(conn)
                    .await
                    .unwrap())
            }
        }
    }
}