use async_trait::async_trait;
use crate::SquadOvError;
use serde::Deserialize;
use lapin::{
    BasicProperties,
    Connection,
    ConnectionProperties,
    Channel,
    options::{QueueDeclareOptions, BasicConsumeOptions, BasicPublishOptions, BasicAckOptions, BasicQosOptions},
    types::FieldTable,
    Consumer,
};
use futures_util::stream::StreamExt;
use async_std::sync::{Arc, RwLock};
use std::collections::{HashMap, VecDeque};

pub const RABBITMQ_DEFAULT_PRIORITY: u8 = 0;
pub const RABBITMQ_HIGH_PRIORITY: u8 = 10;
const RABBITMQ_PREFETCH_COUNT: u16 = 2;

#[derive(Deserialize,Debug,Clone)]
pub struct RabbitMqConfig {
    pub amqp_url: String,
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
}

#[async_trait]
pub trait RabbitMqListener: Send + Sync {
    async fn handle(&self, data: &[u8]) -> Result<(), SquadOvError>;
}

pub struct RabbitMqConnectionBundle {
    channels: Vec<Channel>,
    listeners: Arc<RwLock<HashMap<String, Vec<Arc<dyn RabbitMqListener>>>>>,
}

#[derive(Debug)]
pub struct RabbitMqPacket {
    queue: String,
    data: Vec<u8>,
    priority: u8,
}

pub struct RabbitMqInterface {
    pub config: RabbitMqConfig,
    publish_queue: Arc<RwLock<VecDeque<RabbitMqPacket>>>,
    // Each queue gets its own vector of consumers so that we allocate 1 thread
    // per connection bundle and we allow having multiple threads all receiving
    // work from a single queue.
    consumers: RwLock<HashMap<String, Vec<Arc<RabbitMqConnectionBundle>>>>,
}

type RequeueCallbackFn = fn(&RabbitMqInterface, RabbitMqPacket, i64);

impl RabbitMqConnectionBundle {
    fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub async fn connect(config: &RabbitMqConfig, num_channels: i32) -> Result<Self, SquadOvError> {
        let connection = Connection::connect(
            &config.amqp_url,
            ConnectionProperties::default()
        ).await?;

        let mut channels: Vec<Channel> = Vec::new();
        for _i in 0..num_channels {
            let ch = connection.create_channel().await?;
            ch.queue_declare(
                &config.rso_queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            ).await?;

            ch.queue_declare(
                &config.valorant_queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            ).await?;

            ch.queue_declare(
                &config.lol_queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            ).await?;

            ch.queue_declare(
                &config.tft_queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            ).await?;

            ch.queue_declare(
                &config.vod_queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            ).await?;

            channels.push(ch);
        }

        Ok(Self {
            channels,
            listeners: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn publish(&self, msg: RabbitMqPacket, ch_idx: usize) -> Result<(), SquadOvError> {
        let ch = self.channels.get(ch_idx);
        if ch.is_none() {
            return Err(SquadOvError::BadRequest)
        }

        let ch = ch.unwrap();
        ch.basic_publish(
            "",
            &msg.queue,
            BasicPublishOptions::default(),
            msg.data,
            BasicProperties::default()
                .with_priority(msg.priority),
        ).await?.await?;

        Ok(())
    }

    fn start_consumer(&self, queue: &str, mut consumer: Consumer, itf: Arc<RabbitMqInterface>, requeue_callback: RequeueCallbackFn) {
        let queue = String::from(queue);
        let listeners = self.listeners.clone();
        tokio::task::spawn(async move {
            while let Some(msg) = consumer.next().await {
                if msg.is_err() {
                    log::warn!("Failed to consume from RabbitMQ: {:?}", msg.err().unwrap());
                    continue;
                }

                let msg = msg.unwrap();
                let current_listeners = listeners.read().await.clone();
                let topic_listeners = current_listeners.get(&queue);
                let mut requeue_ms: Option<i64> = None;
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
                                    _ => {},
                                }
                            },
                        };
                    }    
                }

                match msg.acker.ack(BasicAckOptions::default()).await {
                    Ok(_) => (),
                    Err(err) => log::warn!("Failed to ack RabbitMQ message: {:?}", err)
                };

                if requeue_ms.is_some() {
                    requeue_callback(&*itf, RabbitMqPacket{
                        queue: queue.clone(),
                        data: msg.data.clone(),
                        priority: msg.properties.priority().unwrap_or(RABBITMQ_DEFAULT_PRIORITY), 
                    }, requeue_ms.unwrap());
                }
            }
        });
    }

    pub async fn begin_consuming(&self, itf: Arc<RabbitMqInterface>, queue: &str, requeue_callback: RequeueCallbackFn) -> Result<(), SquadOvError> {
        // Each channel gets its own thread to start consuming from every channel.
        // I think we should probably only limit ourselves to having 1 consumer channel anyway.
        for ch in &self.channels {
            ch.basic_qos(RABBITMQ_PREFETCH_COUNT, BasicQosOptions::default()).await?;

            let consumer = ch.basic_consume(
                queue,
                "",
                BasicConsumeOptions::default(),
                FieldTable::default(),

            ).await?;
            self.start_consumer(queue, consumer, itf.clone(), requeue_callback);
        }

        Ok(())
    }
}

impl RabbitMqInterface {
    pub async fn new(config: &RabbitMqConfig, enabled: bool) -> Result<Arc<Self>, SquadOvError> {
        log::info!("Connecting to RabbitMQ...");
        let publisher = Arc::new(RabbitMqConnectionBundle::connect(&config, if enabled { 4 } else { 0 }).await?);
        let publish_queue = Arc::new(RwLock::new(VecDeque::new()));

        if enabled {
            log::info!("\tStart Publishing (RabbitMQ)...");
            {
                let publisher = publisher.clone();
                let publish_queue = publish_queue.clone();
                tokio::task::spawn(async move {
                    let mut publish_idx: usize = 0;
                    loop {
                        {
                            let next_msg = publish_queue.write().await.pop_front();
                            if next_msg.is_some() {
                                let next_msg = next_msg.unwrap();
                                match publisher.publish(next_msg, publish_idx).await {
                                    Ok(_) => (),
                                    Err(err) => log::warn!("Failed to publish RabbitMQ message: {:?}", err)
                                }
                                publish_idx = (publish_idx + 1) % publisher.num_channels();
                            }
                        }
                        async_std::task::sleep(std::time::Duration::from_millis(1)).await;
                    }
                });
            }
        }

        let itf = Arc::new(Self {
            config: config.clone(),
            publish_queue,
            consumers: RwLock::new(HashMap::new()),
        });
        log::info!("RabbitMQ Successfully Connected");
        Ok(itf)
    }

    fn publish_delay(&self, packet: RabbitMqPacket, time_ms: i64) {
        let queue = self.publish_queue.clone();
        tokio::task::spawn(async move {
            async_std::task::sleep(std::time::Duration::from_millis(time_ms as u64)).await;
            queue.write().await.push_back(packet);
        });
    }

    pub async fn add_listener(itf: Arc<RabbitMqInterface>, queue: String, listener: Arc<dyn RabbitMqListener>) -> Result<(), SquadOvError> {
        let mut consumers = itf.consumers.write().await;
        if !consumers.contains_key(&queue) {
            consumers.insert(queue.clone(), Vec::new());
        }

        let consumer_array = consumers.get_mut(&queue).unwrap();
        log::info!("Start Consuming (RabbitMQ) on Queue {} [{}]...", &queue, consumer_array.len());

        let consumer = Arc::new(RabbitMqConnectionBundle::connect(&itf.config, 1).await?);
        
        {
            let mut all_listeners = consumer.listeners.write().await;
            if !all_listeners.contains_key(&queue) {
                all_listeners.insert(queue.clone(), Vec::new());
            }

            let arr = all_listeners.get_mut(&queue).unwrap();
            arr.push(listener);
        }

        consumer_array.push(consumer.clone());
        consumer.begin_consuming(itf.clone(), &queue, RabbitMqInterface::publish_delay).await?;
        log::info!("\t...Success.");
        Ok(())
    }

    pub async fn publish(&self, queue: &str, data: Vec<u8>, priority: u8) {
        self.publish_queue.write().await.push_back(RabbitMqPacket{
            queue: String::from(queue),
            data,
            priority
        });
    }
}