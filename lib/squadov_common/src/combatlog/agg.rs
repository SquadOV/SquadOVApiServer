pub mod sliding_window;

use crate::SquadOvError;
use chrono::{DateTime, Utc};
use serde::Serialize;

pub struct InputAggregatorPacket<T> {
    pub tm: DateTime<Utc>,
    pub data: T,
}

#[derive(Serialize)]
pub struct OutputAggregatorPacket<T> {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub value: T,
}

pub trait CombatLogAggregator<T> {
    fn handle(&mut self, packet: InputAggregatorPacket<T>) -> Result<Option<OutputAggregatorPacket<T>>, SquadOvError>;
    fn flush(&mut self) -> Result<Option<OutputAggregatorPacket<T>>, SquadOvError>;
}