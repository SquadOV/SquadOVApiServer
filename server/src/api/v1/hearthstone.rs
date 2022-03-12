mod arena;
mod cards;
mod create;
mod deck;
mod list;
mod get;
mod duels;

use serde::{Deserialize};
use uuid::Uuid;
use squadov_common::hearthstone::GameType;

#[derive(Deserialize)]
pub struct HearthstoneMatchGetInput {
    user_id: i64,
    match_uuid: Uuid
}

#[derive(Deserialize)]
pub struct HearthstoneCollectionGetInput {
    user_id: i64,
    collection_uuid: Uuid
}

#[derive(Deserialize)]
pub struct HearthstoneUserMatchInput {
    user_id: i64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct HearthstoneListQuery {
    has_vod: Option<bool>,
    game_types: Vec<GameType>,
}

impl Default for HearthstoneListQuery {
    fn default() -> Self {
        Self {
            has_vod: None,
            game_types: vec![],
        }
    }
}

pub use arena::*;
pub use cards::*;
pub use create::*;
pub use deck::*;
pub use list::*;
pub use get::*;
pub use duels::*;