use serde::{Serialize, Deserialize};
use rdkafka::consumer::{stream_consumer::StreamConsumer, Consumer};
use rdkafka::message::{OwnedMessage};
use rdkafka::Message;
use futures_util::StreamExt;
use std::future::Future;
use crate::SquadOvError;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KafkaCredentialKeyPair {
    pub key: String,
    pub secret: String
}

pub async fn generic_kafka_message_loop<F, T>(consumer: StreamConsumer, opaque: T, handler: impl Fn(OwnedMessage, T) -> F)
where
    T: Clone,
    F: Future<Output = Result<bool, SquadOvError>>
{
    let mut message_stream = consumer.stream();
    while let Some(message) = message_stream.next().await {
        match message {
            Err(err) => log::warn!("Kafka Message Stream Err: {}", err),
            Ok(m) => {
                let owned = m.detach();
                match handler(owned, opaque.clone()).await {
                    Ok(commit) => {
                        if commit {
                            match consumer.store_offset_from_message(&m) {
                                Ok(_) => (),
                                Err(e) => log::warn!("Failure to store Kafka offset in topic [{}]: {}", m.topic(), e)
                            }
                        }
                    },
                    Err(err) => {
                        log::warn!("Error in handling Kafka message in topic [{}]: {}", m.topic(), err);
                    }
                }
            }
        }
    }
}