#[macro_use]
extern crate log;

mod api;

use structopt::StructOpt;
use std::{fs, sync::Arc};
use squadov_common::SquadOvError;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: std::path::PathBuf,
    #[structopt(short, long)]
    db: u32,
}

#[tokio::main]
pub async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,singleton_event_processing_worker=debug,actix_web=debug,actix_http=debug,librdkafka=info,rdkafka::client=info");
    std::env::set_var("SQLX_LOG", "0");
    env_logger::init();

    let opts = Options::from_args();
    let raw_cfg = fs::read_to_string(opts.config.clone()).unwrap();
    let mut config : api::ApiConfig = toml::from_str(&raw_cfg).unwrap();
    config.database.connections = opts.db;
    config.database.heavy_connections = opts.db;
    config.rabbitmq.enable_rso = true;
    config.rabbitmq.enable_lol = true;
    config.rabbitmq.enable_tft = true;
    config.rabbitmq.enable_valorant = true;
    config.rabbitmq.enable_vod = false;
    config.rabbitmq.enable_csgo = false;
    config.rabbitmq.enable_steam = true;
    config.rabbitmq.enable_twitch = true;
    config.rabbitmq.enable_sharing = false;
    config.rabbitmq.prefetch_count = 2;

    // Only use the provided config to connect to things.
    let _ = tokio::task::spawn(async move {
        let app = Arc::new(api::ApiApplication::new(&config).await);
        api::start_event_loop(app.clone());

        loop {
            async_std::task::sleep(std::time::Duration::from_secs(10)).await;
        }
    }).await;
    Ok(())
}