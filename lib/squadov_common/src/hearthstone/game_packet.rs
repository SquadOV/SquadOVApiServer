use serde::Serialize;
use chrono::{DateTime, Utc};
use crate::hearthstone::game_state::{HearthstoneGameBlock, HearthstoneGameSnapshot, HearthstoneGameAction};
use crate::hearthstone::{GameType, FormatType, HearthstoneDeck, HearthstonePlayer};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Serialize)]
pub struct HearthstoneMatchMetadata {
    #[serde(rename = "gameType")]
    pub game_type: GameType,
    #[serde(rename = "elapsedSeconds")]
    pub elapsed_seconds: f64,
    #[serde(rename = "formatType")]
    pub format_type: FormatType,
    #[serde(rename = "scenarioId")]
    pub scenario_id: i32,
    #[serde(rename = "matchTime")]
    pub match_time: DateTime<Utc>,
    pub deck: Option<HearthstoneDeck>,
    pub players: HashMap<i32, HearthstonePlayer>
}

#[derive(Serialize)]
pub struct HearthstoneGameLogMetadata {
    #[serde(rename = "snapshotIds")]
    pub snapshot_ids: Vec<Uuid>,
    #[serde(rename = "numActions")]
    pub num_actions: i64
}

#[derive(Serialize)]
pub struct HearthstoneSerializedGameLog{
    pub snapshots: Vec<HearthstoneGameSnapshot>,
    pub actions: Vec<HearthstoneGameAction>,
    pub blocks: Vec<HearthstoneGameBlock>
}

#[derive(Serialize)]
pub struct HearthstoneGamePacket {
    #[serde(rename = "matchUuid")]
    pub match_uuid: Uuid,
    pub metadata: HearthstoneMatchMetadata,
    #[serde(rename = "logMetadata")]
    pub log_metadata: HearthstoneGameLogMetadata,
    #[serde(rename = "latestSnapshot")]
    pub latest_snapshot: Option<HearthstoneGameSnapshot>
}