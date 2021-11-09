#[macro_use]
extern crate log;

mod api;

use structopt::StructOpt;
use std::fs;
use squadov_common::SquadOvError;
use uuid::Uuid;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: std::path::PathBuf,
    #[structopt(short, long)]
    db: u32,
    #[structopt(short, long)]
    threads: i32,
    #[structopt(short, long)]
    vod: Option<Uuid>,
}

#[tokio::main]
pub async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,vod_processing_worker=debug,actix_web=debug,actix_http=debug,librdkafka=info,rdkafka::client=info");
    std::env::set_var("SQLX_LOG", "0");
    env_logger::init();

    let opts = Options::from_args();
    let raw_cfg = fs::read_to_string(opts.config.clone()).unwrap();
    let mut config : api::ApiConfig = toml::from_str(&raw_cfg).unwrap();
    config.vod.fastify_threads = opts.threads;
    config.database.connections = opts.db;
    config.database.heavy_connections = opts.db;
    config.rabbitmq.enable_rso = false;
    config.rabbitmq.enable_lol = false;
    config.rabbitmq.enable_tft = false;
    config.rabbitmq.enable_valorant = false;
    config.rabbitmq.enable_vod = true;
    config.rabbitmq.enable_csgo = false;
    config.rabbitmq.enable_steam = false;
    config.rabbitmq.enable_twitch = false;
    config.rabbitmq.prefetch_count = 1;

    // Only use the provided config to connect to things.
    let _ = tokio::task::spawn(async move {
        let app = api::ApiApplication::new(&config).await;
        if let Some(vod) = opts.vod {
            app.vod_itf.process_vod(&vod, "source", None).await.unwrap();
        } else {
            loop {
                async_std::task::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }).await;
    Ok(())
}