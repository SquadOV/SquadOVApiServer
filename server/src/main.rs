#[macro_use]
extern crate log;

mod api;
mod wow_kafka;

use tokio;
use actix_rt;
use actix_web::{http, App, HttpServer, web};
use actix_web::middleware::{Logger, Compress};
use api::api_service;
use structopt::StructOpt;
use actix_cors::{Cors};
use std::fs;
use rdkafka::config::ClientConfig;
use async_std::sync::{Arc};
use tokio::{
    runtime::Builder,
};
use squadov_common::{
    config::CommonConfig,
};

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: std::path::PathBuf,
    #[structopt(short, long)]
    mode: Option<String>,
    #[structopt(short, long)]
    workers: usize,
}

fn main() {
    let opts = Options::from_args();
    actix_rt::System::with_tokio_rt(|| {
        Builder::new_multi_thread()
            .enable_all()
            .worker_threads(opts.workers)
            .build()
            .unwrap()
    })
        .block_on(async_main(opts));
}

async fn async_main(opts: Options) {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,squadov_api_server=debug,actix_web=debug,actix_http=debug,librdkafka=info,rdkafka::client=info,sqlx=info");
    env_logger::init();

    log::info!("Start SquadOV Api Server.");
    
    let raw_cfg = fs::read_to_string(opts.config).unwrap();
    let mut config : api::ApiConfig = toml::from_str(&raw_cfg).unwrap();
    config.read_from_env();

    let mut kafka_config = ClientConfig::new();
    kafka_config.set("bootstrap.servers", &config.kafka.bootstrap_servers);
    kafka_config.set("security.protocol", "SASL_SSL");
    kafka_config.set("sasl.mechanisms", "PLAIN");
    kafka_config.set("sasl.username", &config.kafka.server_keypair.key);
    kafka_config.set("sasl.password", &config.kafka.server_keypair.secret);
    kafka_config.set("enable.auto.offset.store", "false");

    let app = Arc::new(api::ApiApplication::new(&config, "api").await);

    // A hacky way of doing things related to api::ApiApplication...
    if opts.mode.is_some() {
        let mode = opts.mode.unwrap();
        if mode == "vod_fastify" {
            let vods = app.find_vods_without_fastify().await.unwrap();
            for v in vods {
                log::info!("Enqueue job: {}", &v);
                app.vod_itf.request_vod_processing(&v, "source", None, false).await.unwrap();
            }
            async_std::task::sleep(std::time::Duration::from_secs(5)).await;
        } else if mode == "vod_preview" {
            let vods = app.find_vods_without_preview().await.unwrap();
            for v in vods {
                log::info!("Enqueue job: {}", &v);
                app.vod_itf.request_vod_processing(&v, "source", None, false).await.unwrap();
            }
            async_std::task::sleep(std::time::Duration::from_secs(5)).await;
        } else if mode == "wow_manual_parsing" {
            wow_kafka::create_wow_consumer_thread(app.clone(), &config.kafka.wow_combat_log_topic, &kafka_config).await.unwrap();
        } else {
            log::error!("Invalid mode: {}", &mode);
        }
    } else {
        let config2 = config.clone();

        for _i in 0..config.kafka.wow_combat_log_threads {
            wow_kafka::create_wow_consumer_thread(app.clone(), &config.kafka.wow_combat_log_topic, &kafka_config);
        }

        let redis_pool = Arc::new(deadpool_redis::Config{
            url: Some(config.redis.url.clone()),
            pool: Some(deadpool::managed::PoolConfig{
                max_size: config.redis.pool_size,
                timeouts: deadpool::managed::Timeouts{
                    wait: Some(std::time::Duration::from_millis(config.redis.timeout_ms)),
                    create: Some(std::time::Duration::from_millis(config.redis.timeout_ms)),
                    recycle: Some(std::time::Duration::from_millis(config.redis.timeout_ms)),
                },
            }),
            connection: None,
        }.create_pool(Some(deadpool_redis::Runtime::Tokio1)).unwrap());

        let user_status_tracker = squadov_common::squad::status::UserActivityStatusTracker::new(&config.redis, redis_pool.clone()).await;
        
        // The API service is primarily used for dealing with API calls.actix_web
        // We're not going to have a web-based interface at the moment (only going to be desktop client-based)
        // so this server doesn't have to serve javascript or the like.
        HttpServer::new(move || {
            App::new()
                .wrap(Compress::default())
                .wrap(
                    Cors::default()
                        .allowed_origin(&config.cors.domain)
                        .allowed_origin("http://127.0.0.1:8080")
                        .allowed_origin("https://www.squadov.gg")
                        .allowed_origin_fn(|_origin, req| {
                            req.headers
                                .get(http::header::ORIGIN)
                                .map(http::header::HeaderValue::as_bytes)
                                .filter(|b| b == b"file://")
                                .is_some()
                        })
                        .allowed_methods(vec!["GET", "POST", "OPTIONS", "DELETE", "PUT"])
                        .allowed_headers(vec![
                            "x-squadov-access-token",
                            "x-squadov-session-id",
                            "x-squadov-share-id",
                            "content-type",
                            "pragma",
                            "cache-control",
                        ])
                )
                .wrap(Logger::default())
                .app_data(web::Data::new(user_status_tracker.clone()))
                .app_data(web::Data::new(app.clone()))
                .service(api_service::create_service(config.server.graphql_debug))
            })
            .workers(config2.server.workers)
            .server_hostname(&config2.server.domain)
            .bind("0.0.0.0:8080")
            .unwrap()
            .run()
            .await
            .unwrap();
    }
}