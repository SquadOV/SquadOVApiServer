use squadov_common::{
    SquadOvError,
    rabbitmq::{RabbitMqInterface, RabbitMqConfig, RabbitMqPacket},
};
use structopt::StructOpt;
use std::fs;
use std::sync::Arc;
use serde::Deserialize;
use sqlx::{
    postgres::{
        PgPool,
        PgListener,
        PgPoolOptions
    },
};
use chrono::{DateTime, Utc};

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    config: String,
}

#[derive(Deserialize,Debug,Clone)]
struct Config {
    db: String,
    connections: u32,
    rabbitmq: RabbitMqConfig,
}

const PG_TOPIC_RABBITMQ_DELAY: &'static str = "rabbitmq_delay";

struct Worker {
    pool: Arc<PgPool>,
    listener: PgListener,
    rmq: Arc<RabbitMqInterface>
}

#[derive(Deserialize)]
struct DelayedMessage {
    id: i64,
    execute_time: DateTime<Utc>
}

impl Worker {
    fn new(pool: Arc<PgPool>, listener: PgListener, rmq: Arc<RabbitMqInterface>) -> Self {
        Self {
            pool,
            listener,
            rmq,
        }
    }

    async fn initialize(&self) -> Result<(), SquadOvError> {
        let existing_messages = sqlx::query_as!(
            DelayedMessage,
            "
            SELECT id, execute_time
            FROM squadov.deferred_rabbitmq_messages
            "
        )
            .fetch_all(&*self.pool)
            .await?;

        for m in existing_messages {
            self.spawn_task_for_delayed_message(&m).await?;
        }

        Ok(())
    }

    async fn spawn_task_for_delayed_message(&self, msg: &DelayedMessage) -> Result<(), SquadOvError> {
        let now = Utc::now();
        let diff_ms = (msg.execute_time - now).num_milliseconds();
        let msg_id = msg.id;
        let pool = self.pool.clone();
        let rmq = self.rmq.clone();
        tokio::task::spawn(async move {
            if diff_ms > 0 {
                log::info!("RabbitMQ Task Sleep for {}ms", diff_ms);
                async_std::task::sleep(std::time::Duration::from_millis(diff_ms as u64)).await;
            }

            log::info!("Redirect RabbitMQ Task: {}", msg_id);
            let data = match sqlx::query!(
                r#"
                DELETE FROM squadov.deferred_rabbitmq_messages
                WHERE id = $1
                RETURNING message                
                "#,
                msg_id
            )
                .fetch_optional(&*pool)
                .await {
                Ok(x) => x.map(|y| {
                    y.message
                }),
                Err(_) => None,
            };

            if data.is_some() {
                let mut parsed: RabbitMqPacket = serde_json::from_slice(&data.unwrap()).unwrap();
                parsed.base_delay_ms = None;
                rmq.publish_direct(parsed);
            } else {
                log::info!("Skipping message {} as it no longer exists or we could no longer access it?", msg_id);
            }
        });
        Ok(())
    }

    async fn run(mut self) -> Result<(), SquadOvError> {
        loop {
            let notification = self.listener.recv().await?;
            println!("Forwarding Delayed RabbitMQ Message: {:?}", notification);

            let msg = serde_json::from_str(notification.payload())?;
            self.spawn_task_for_delayed_message(&msg).await?;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,rabbitmq_delay_handler=debug");
    std::env::set_var("SQLX_LOG", "0");
    env_logger::init();

    let opts = Options::from_args();
    let raw_cfg = fs::read_to_string(opts.config).unwrap();
    let config : Config = toml::from_str(&raw_cfg).unwrap();

    tokio::task::spawn(async move {
        let pool = Arc::new(PgPoolOptions::new()
            .min_connections(1)
            .max_connections(config.connections)
            .max_lifetime(std::time::Duration::from_secs(6*60*60))
            .idle_timeout(std::time::Duration::from_secs(3*60*60))
            .connect(&config.db)
            .await
            .unwrap());
        let mut listener = PgListener::connect(&config.db).await.unwrap();
        listener.listen_all(vec![PG_TOPIC_RABBITMQ_DELAY]).await.unwrap();

        let rabbitmq = RabbitMqInterface::new(&config.rabbitmq, pool.clone(), true).await.unwrap();
        let worker = Worker::new(pool, listener, rabbitmq);
        worker.initialize().await.unwrap();
        worker.run().await.unwrap();
    }).await.unwrap();

    Ok(())
}