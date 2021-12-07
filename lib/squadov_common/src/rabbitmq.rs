use async_trait::async_trait;
use crate::SquadOvError;
use serde::{Serialize, Deserialize};
use lapin::{
    BasicProperties,
    Connection,
    ConnectionProperties,
    Channel,
    options::{QueueDeclareOptions, BasicConsumeOptions, BasicPublishOptions, BasicAckOptions, BasicQosOptions},
    types::{FieldTable, AMQPValue, ShortString},
    Consumer,
};
use futures_util::stream::StreamExt;
use async_std::sync::{Arc, RwLock};
use std::collections::{HashMap, VecDeque};
use chrono::{DateTime, Utc, NaiveDateTime};
use rand::Rng;
use sqlx::PgPool;

pub const RABBITMQ_DEFAULT_PRIORITY: u8 = 0;
pub const RABBITMQ_HIGH_PRIORITY: u8 = 5;
const RABBITMQ_MAX_DELAY_MS: i64 = 7200000; // 2 hour
const SQUADOV_RETRY_COUNT_HEADER: &'static str = "x-squadov-retry-count";
const SQUADOV_MESSAGE_MAX_AGE_HEADER: &'static str = "x-squadov-max-age";
const DEFAULT_MAX_AGE_SECONDS: i64 = 3600; // 1 hour
const INFITE_MAX_AGE: i64 = -1;

#[derive(Deserialize,Debug,Clone)]
pub struct RabbitMqConfig {
    pub amqp_url: String,
    pub prefetch_count: u16,
    pub enable_rso: bool,
    pub rso_queue: String,
    pub enable_valorant: bool,
    pub valorant_queue: String,
    pub enable_lol: bool,
    pub lol_queue: String,
    pub enable_tft: bool,
    pub tft_queue: String,
    pub enable_vod: bool,
    pub vod_queue: String,
    pub enable_csgo: bool,
    pub csgo_queue: String,
    pub enable_steam: bool,
    pub steam_queue: String,
    pub enable_twitch: bool,
    pub twitch_queue: String,
    pub enable_sharing: bool,
    pub sharing_queue: String,
    pub additional_queues: Option<Vec<String>>,
}

#[async_trait]
pub trait RabbitMqListener: Send + Sync {
    async fn handle(&self, data: &[u8]) -> Result<(), SquadOvError>;
}

pub struct RabbitMqConnectionBundle {
    channels: Vec<Channel>,
    listeners: Arc<RwLock<HashMap<String, Vec<Arc<dyn RabbitMqListener>>>>>,
    db: Arc<PgPool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RabbitMqPacket {
    queue: String,
    data: Vec<u8>,
    priority: u8,
    timestamp: DateTime<Utc>,
    retry_count: u32,
    pub base_delay_ms: Option<i64>,
    max_age_seconds: i64,
}

pub struct RabbitMqInterface {
    pub config: RabbitMqConfig,
    publish_queue: Arc<RwLock<VecDeque<RabbitMqPacket>>>,
    db: Arc<PgPool>,
}

type RequeueCallbackFn = fn(&RabbitMqInterface, RabbitMqPacket);

impl RabbitMqConnectionBundle {
    fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub async fn connect(config: &RabbitMqConfig, db: Arc<PgPool>, num_channels: i32) -> Result<Self, SquadOvError> {
        let connection = Connection::connect(
            &config.amqp_url,
            ConnectionProperties::default()
        ).await?;

        let mut default_table = FieldTable::default();
        default_table.insert(ShortString::from("x-max-priority"), AMQPValue::LongUInt(10));

        let queue_opts = QueueDeclareOptions{
            passive: false,
            durable: true,
            exclusive: false,
            auto_delete: false,
            nowait: false,
        };

        let mut channels: Vec<Channel> = Vec::new();
        for _i in 0..num_channels {
            let ch = connection.create_channel().await?;

            if !config.rso_queue.is_empty() {
                ch.queue_declare(
                    &config.rso_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.valorant_queue.is_empty() {
                ch.queue_declare(
                    &config.valorant_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.lol_queue.is_empty() {
                ch.queue_declare(
                    &config.lol_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.tft_queue.is_empty() {
                ch.queue_declare(
                    &config.tft_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.vod_queue.is_empty() {
                ch.queue_declare(
                    &config.vod_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.csgo_queue.is_empty() {
                ch.queue_declare(
                    &config.csgo_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.steam_queue.is_empty() {
                ch.queue_declare(
                    &config.steam_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.twitch_queue.is_empty() {
                ch.queue_declare(
                    &config.twitch_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if !config.sharing_queue.is_empty() {
                ch.queue_declare(
                    &config.sharing_queue,
                    queue_opts.clone(),
                    default_table.clone(),
                ).await?;
            }

            if let Some(addtl) = &config.additional_queues {
                for q in addtl {
                    ch.queue_declare(
                        q,
                        queue_opts.clone(),
                        default_table.clone(),
                    ).await?;
                }
            }

            channels.push(ch);
        }

        Ok(Self {
            channels,
            db,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn publish(&self, msg: RabbitMqPacket, ch_idx: usize) -> Result<(), SquadOvError> {
        let ch = self.channels.get(ch_idx);
        if ch.is_none() {
            log::warn!("Invalid Channel Index: {}", ch_idx);
            return Err(SquadOvError::BadRequest)
        }

        let mut headers = FieldTable::default();
        headers.insert(ShortString::from(SQUADOV_RETRY_COUNT_HEADER), AMQPValue::LongUInt(msg.retry_count));
        headers.insert(ShortString::from(SQUADOV_MESSAGE_MAX_AGE_HEADER), AMQPValue::LongLongInt(msg.max_age_seconds));

        let ch = ch.unwrap();
        if msg.base_delay_ms.is_none() {
            ch.basic_publish(
                "",
                &msg.queue,
                BasicPublishOptions::default(),
                &msg.data,
                BasicProperties::default()
                    .with_priority(msg.priority)
                    .with_timestamp(msg.timestamp.timestamp() as u64)
                    .with_headers(headers),
            ).await?.await?;
        } else {
            let total_delay_ms = {
                let mut rng = rand::thread_rng();
                let ms = std::cmp::min(
                    2i64.pow(msg.retry_count) +  rng.gen_range(0..1000) + msg.base_delay_ms.unwrap(),
                    RABBITMQ_MAX_DELAY_MS
                );

                // If we get to a negative delay because of integer overflow due to the pow we should just assume
                // that we want to do the max delay.
                if ms <= 0 {
                    RABBITMQ_MAX_DELAY_MS
                } else {
                    ms
                }
            };
            log::info!("Delaying RabbitMQ message for {}ms [Retry {}, Base {:?}].", total_delay_ms, msg.retry_count, msg.base_delay_ms);
            self.add_delayed_rabbitmq_message(msg, total_delay_ms).await?;
        }

        Ok(())
    }

    async fn add_delayed_rabbitmq_message(&self, msg: RabbitMqPacket, total_delay_ms: i64) -> Result<(), SquadOvError> {
        let execute_time = Utc::now() + chrono::Duration::milliseconds(total_delay_ms);
        sqlx::query!(
            "
            INSERT INTO squadov.deferred_rabbitmq_messages (
                execute_time,
                message
            )
            VALUES (
                $1,
                $2
            )
            ",
            execute_time,
            serde_json::to_vec(&msg)?,
        )
            .execute(&*self.db)
            .await?;
        Ok(())
    }

    async fn start_consumer(&self, queue: &str, mut consumer: Consumer, itf: Arc<RabbitMqInterface>, requeue_callback: RequeueCallbackFn) -> Result<(), SquadOvError> {
        let queue = String::from(queue);
        let listeners = self.listeners.clone();

        while let Some(msg) = consumer.next().await {
            if msg.is_err() {
                return Err(SquadOvError::InternalError(format!("Failed to consume from RabbitMQ: {:?}", msg.err().unwrap())));
            }

            let msg = msg.unwrap();

            // Check the application defined max age. If we're past the max age of this particular message then
            // we'd want to discard the message.
            let current_timestamp = Utc::now().timestamp() as u64;
            let og_timestamp = msg.properties.timestamp().unwrap_or(current_timestamp);
            let max_age_seconds = match msg.properties.headers() {
                Some(h) => h.inner().get(&ShortString::from(SQUADOV_MESSAGE_MAX_AGE_HEADER)),
                None => None,
            }.map(|x| {
                match x {
                    AMQPValue::LongLongInt(y) => *y,
                    _ => {
                        log::warn!("Max age header in an unexpected format: {:?}", x);
                        DEFAULT_MAX_AGE_SECONDS
                    },
                }
            }).unwrap_or(DEFAULT_MAX_AGE_SECONDS);

            let expired = (current_timestamp - og_timestamp) > (max_age_seconds as u64) && max_age_seconds != INFITE_MAX_AGE;
            let mut requeue_ms: Option<i64> = None;

            if !expired {
                let current_listeners = listeners.read().await.clone();
                let topic_listeners = current_listeners.get(&queue);
                if topic_listeners.is_some() {
                    let topic_listeners = topic_listeners.unwrap();
                    for l in topic_listeners {
                        match l.handle(&msg.data).await {
                            Ok(_) => (),
                            Err(err) => {
                                log::warn!("Failure in processing RabbitMQ message: {:?}", err);
                                match err {
                                    SquadOvError::Defer(ms) => { requeue_ms = Some(ms); },
                                    SquadOvError::RateLimit => { requeue_ms = Some(100); },
                                    _ => (),
                                }
                            },
                        };
                    }    
                }
            } else {
                log::warn!("Ignoring message because it expired: {:?}", &msg);
            }

            match msg.acker.ack(BasicAckOptions::default()).await {
                Ok(_) => (),
                Err(err) => {
                    return Err(SquadOvError::InternalError(format!("Failed to ack RabbitMQ message: {:?}", err)));
                }
            };

            if requeue_ms.is_some() {
                let retry_count = match msg.properties.headers() {
                    Some(h) => h.inner().get(&ShortString::from(SQUADOV_RETRY_COUNT_HEADER)),
                    None => None,
                }.map(|x| {
                    match x {
                        AMQPValue::LongUInt(y) => *y,
                        _ => {
                            log::warn!("Retry header in an unexpected format: {:?}", x);
                            0
                        },
                    }
                }).unwrap_or(0);

                requeue_callback(&*itf, RabbitMqPacket{
                    queue: queue.clone(),
                    data: msg.data.clone(),
                    priority: msg.properties.priority().unwrap_or(RABBITMQ_DEFAULT_PRIORITY),
                    timestamp: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(og_timestamp as i64, 0), Utc),
                    retry_count: retry_count + 1,
                    base_delay_ms: requeue_ms,
                    max_age_seconds,
                });
            }
        }

        Ok(())
    }

    pub async fn begin_consuming(&self, itf: Arc<RabbitMqInterface>, queue: &str, requeue_callback: RequeueCallbackFn, prefetch_count: u16) -> Result<(), SquadOvError> {
        // Each channel gets its own thread to start consuming from every channel.
        // I think we should probably only limit ourselves to having 1 consumer channel anyway.
        if self.channels.len() != 1 {
            log::warn!("WE SHOULD ONLY BE USING A SINGLE CHANNEL FOR CONSUMERS.");
            return Err(SquadOvError::BadRequest);
        }

        if let Some(ch) = self.channels.first() {
            ch.basic_qos(prefetch_count, BasicQosOptions::default()).await?;

            let consumer = ch.basic_consume(
                queue,
                "",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            ).await?;

            self.start_consumer(queue, consumer, itf.clone(), requeue_callback).await?;
        }

        Ok(())
    }
}

impl RabbitMqInterface {
    pub async fn new(config: &RabbitMqConfig, db: Arc<PgPool>, enabled: bool) -> Result<Arc<Self>, SquadOvError> {
        log::info!("Connecting to RabbitMQ...");
        let publish_queue = Arc::new(RwLock::new(VecDeque::new()));

        if enabled {
            log::info!("\tStart Publishing (RabbitMQ)...");
            {
                let publish_queue = publish_queue.clone();
                let config = config.clone();
                let db = db.clone();
                tokio::task::spawn(async move {
                    loop {
                        log::info!("Connecting to RabbitMQ publisher...");
                        // We need to automatically reconnect if we lose the connection to RabbitMQ.
                        let publisher = match RabbitMqConnectionBundle::connect(&config, db.clone(), if enabled { 4 } else { 0 }).await {
                            Ok(bundle) => Arc::new(bundle),
                            Err(err) => {
                                log::warn!("Failed to connect to RabbitMQ publisher: {:?}", err);
                                async_std::task::sleep(std::time::Duration::from_millis(16)).await;
                                continue;
                            }
                        };

                        log::info!("...Connected to RabbitMQ publisher.");

                        let mut publish_idx: usize = 0;
                        let mut is_valid: bool = true;
                        while is_valid {
                            let mut queue_clone = {
                                let mut queue_lock = publish_queue.write().await;
                                let ret = queue_lock.clone();
                                queue_lock.clear();
                                ret
                            };

                            if !queue_clone.is_empty() {
                                log::info!("RabbitMQ Publishing {} messages", queue_clone.len());
                            }

                            while !queue_clone.is_empty() {
                                let next_msg = queue_clone.pop_front();
                                if next_msg.is_some() {
                                    let next_msg = next_msg.unwrap();
                                    match publisher.publish(next_msg, publish_idx).await {
                                        Ok(_) => (),
                                        Err(err) => {
                                            is_valid = false;
                                            log::warn!("Failed to publish RabbitMQ message: {:?}", err);
                                            break;
                                        }
                                    }
                                    publish_idx = (publish_idx + 1) % publisher.num_channels();
                                }
                            }
                            async_std::task::sleep(std::time::Duration::from_millis(1)).await;
                        }
                    }
                });
            }
        }

        let itf = Arc::new(Self {
            config: config.clone(),
            publish_queue,
            db,
        });
        log::info!("RabbitMQ Successfully Connected");
        Ok(itf)
    }

    pub fn publish_direct(&self, packet: RabbitMqPacket) {
        let queue = self.publish_queue.clone();
        tokio::task::spawn(async move {
            queue.write().await.push_back(packet);
        });
    }

    pub async fn add_listener(itf: Arc<RabbitMqInterface>, queue: String, listener: Arc<dyn RabbitMqListener>, prefetch_count: u16) -> Result<(), SquadOvError> {
        // Need to spawn a management thread - if the connection fails for whatever reason, the connection
        // needs to be remade.
        tokio::task::spawn(async move {
            loop {
                log::info!("Start Consuming (RabbitMQ) on Queue {}...", &queue);
                let consumer = match RabbitMqConnectionBundle::connect(&itf.config, itf.db.clone(), 1).await {
                    Ok(bundle) => Arc::new(bundle),
                    Err(err) => {
                        log::warn!("Failed to connect to RabbitMQ consumer: {:?}", err);
                        async_std::task::sleep(std::time::Duration::from_millis(16)).await;
                        continue;
                    }
                };
                
                {
                    let mut all_listeners = consumer.listeners.write().await;
                    if !all_listeners.contains_key(&queue) {
                        all_listeners.insert(queue.clone(), Vec::new());
                    }

                    let arr = all_listeners.get_mut(&queue).unwrap();
                    arr.push(listener.clone());
                }

                log::info!("\t...Successful start of RabbitMQ consumption.");

                match consumer.begin_consuming(itf.clone(), &queue, RabbitMqInterface::publish_direct, prefetch_count).await {
                    Ok(_) => (),
                    Err(err) => {
                        log::warn!("Failed while RabbitMQ consuming: {:?}", err);
                        async_std::task::sleep(std::time::Duration::from_millis(16)).await;
                        continue;
                    }
                };
            }
        });

        Ok(())
    }

    pub async fn publish(&self, queue: &str, data: Vec<u8>, priority: u8, max_age_seconds: i64) {
        self.publish_queue.write().await.push_back(RabbitMqPacket{
            queue: String::from(queue),
            data,
            priority,
            timestamp: Utc::now(),
            retry_count: 0,
            base_delay_ms: None,
            max_age_seconds,
        });
    }
}