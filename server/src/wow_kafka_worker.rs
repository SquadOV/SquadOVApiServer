#[macro_use]
extern crate log;

mod api;
mod wow_kafka;

use rdkafka::config::ClientConfig;
use std::sync::Arc;
use structopt::StructOpt;
use std::fs;
use squadov_common::SquadOvError;

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: std::path::PathBuf,
    #[structopt(short, long)]
    db: u32,
    #[structopt(short, long)]
    kafka: i32,
    #[structopt(short, long)]
    pg: String,
}

#[tokio::main]
pub async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,wow_kafka_worker=debug,actix_web=debug,actix_http=debug,librdkafka=info,rdkafka::client=info");
    std::env::set_var("SQLX_LOG", "0");
    env_logger::init();

    let opts = Options::from_args();
    let raw_cfg = fs::read_to_string(opts.config).unwrap();
    let mut config : api::ApiConfig = toml::from_str(&raw_cfg).unwrap();
    config.kafka.wow_combat_log_threads = opts.kafka;
    config.database.url = opts.pg;
    config.database.connections = opts.db;
    config.database.heavy_connections = opts.db;

    // Only use the provided config to connect to things.
    let app = Arc::new(api::ApiApplication::new(&config).await);
    let mut kafka_config = ClientConfig::new();
    kafka_config.set("bootstrap.servers", &config.kafka.bootstrap_servers);
    kafka_config.set("security.protocol", "SASL_SSL");
    kafka_config.set("sasl.mechanisms", "PLAIN");
    kafka_config.set("sasl.username", &config.kafka.server_keypair.key);
    kafka_config.set("sasl.password", &config.kafka.server_keypair.secret);
    kafka_config.set("enable.auto.offset.store", "false");

    let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    for i in 0..config.kafka.wow_combat_log_threads {
        log::info!("Spawning Kafka Worker: {}", i);
        tasks.push(wow_kafka::create_wow_consumer_thread(app.clone(), &config.kafka.wow_combat_log_topic, &kafka_config));
    }

    for handle in tasks {
        handle.await.unwrap();
    }

    Ok(())
}