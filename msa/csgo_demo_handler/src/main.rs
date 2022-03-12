use squadov_common::{
    SquadOvError,
    rabbitmq::{RabbitMqInterface, RabbitMqConfig},
    steam::{
        api::{SteamApiConfig, SteamApiClient},
        rabbitmq::SteamApiRabbitmqInterface,
    },
    csgo::rabbitmq::CsgoRabbitmqInterface,
};
use structopt::StructOpt;
use serde::Deserialize;
use std::fs;
use std::sync::Arc;
use sqlx::{
    ConnectOptions,
    postgres::{
        PgPoolOptions,
        PgConnectOptions,
    },
};

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: String,
}

#[derive(Deserialize,Debug,Clone)]
struct Config {
    db_host: String,
    db_username: String,
    db_password: String,
    connections: u32,
    rabbitmq: RabbitMqConfig,
    steam: SteamApiConfig,
}

#[tokio::main]
async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,csgo_demo_handler=debug,sqlx=info");
    env_logger::init();

    let opts = Options::from_args();
    let raw_cfg = fs::read_to_string(opts.config).unwrap();
    let config : Config = toml::from_str(&raw_cfg).unwrap();

    tokio::task::spawn(async move {
        let mut conn = PgConnectOptions::new()
            .host(&config.db_host)
            .username(&config.db_username)
            .password(&config.db_password)
            .port(5432)
            .application_name("csgo_demo_handler")
            .database("squadov")
            .statement_cache_capacity(0);
        conn.log_statements(log::LevelFilter::Trace);
        let pool = Arc::new(PgPoolOptions::new()
            .min_connections(1)
            .max_connections(config.connections)
            .max_lifetime(std::time::Duration::from_secs(6*60*60))
            .idle_timeout(std::time::Duration::from_secs(3*60*60))
            .connect_with(conn)
            .await
            .unwrap());

        let rabbitmq = RabbitMqInterface::new(&config.rabbitmq, Some(pool.clone()), true).await.unwrap();
        let steam_api = Arc::new(SteamApiClient::new(&config.steam));

        let steam_itf = Arc::new(SteamApiRabbitmqInterface::new(steam_api.clone(), &config.rabbitmq, rabbitmq.clone(), pool.clone()));
        let csgo_itf = Arc::new(CsgoRabbitmqInterface::new(steam_itf.clone(), &config.rabbitmq, rabbitmq.clone(), pool.clone()));
        RabbitMqInterface::add_listener(rabbitmq.clone(), config.rabbitmq.csgo_queue.clone(), csgo_itf, config.rabbitmq.prefetch_count).await.unwrap();
        loop {
            async_std::task::sleep(std::time::Duration::from_secs(5)).await;
        }
    }).await.unwrap();

    Ok(())
}