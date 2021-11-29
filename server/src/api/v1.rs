mod user;
mod matches;
mod aimlab;
mod hearthstone;
mod valorant;
mod vod;
mod squad;
mod bug;
mod wow;
mod kafka;
mod lol;
mod tft;
mod oauth;
mod features;
mod analytics;
mod csgo;
mod share;
mod community;
mod profile;
mod sentry;
mod twitch;
mod discord;

pub use user::*;
pub use matches::*;
pub use aimlab::*;
pub use hearthstone::*;
pub use valorant::*;
pub use vod::*;
pub use squad::*;
pub use bug::*;
pub use wow::*;
pub use kafka::*;
pub use lol::*;
pub use tft::*;
pub use oauth::*;
pub use features::*;
pub use analytics::*;
pub use csgo::*;
pub use share::*;
pub use community::*;
pub use profile::*;
pub use sentry::*;
pub use twitch::*;
pub use discord::*;

use serde::Serialize;

#[derive(Serialize)]
pub struct FavoriteResponse {
    favorite: bool,
    reason: Option<String>,
}