mod matches;
mod characters;
mod match_info;
mod stats;

pub use matches::*;
pub use characters::*;
pub use match_info::*;
pub use stats::*;

use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct WoWViewPath {
    pub view_uuid: Uuid
}

#[derive(Deserialize)]
pub struct WoWMatchPath {
    pub match_uuid: Uuid
}

#[derive(Deserialize)]
pub struct WoWUserPath {
    pub user_id: i64
}

#[derive(Deserialize)]
pub struct WoWUserCharacterPath {
    pub user_id: i64,
    pub character_guid: String
}

#[derive(Deserialize)]
pub struct WoWUserMatchPath {
    pub user_id: i64,
    pub match_uuid: Uuid
}

#[derive(Deserialize)]
pub struct WowCombatLogPath {
    pub partition_id: String,
}