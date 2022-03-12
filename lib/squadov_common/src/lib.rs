#[macro_use]
extern crate lazy_static;

pub mod error;
pub mod parse;
pub mod hal;
pub mod vod;
pub mod speed_check;
pub mod gcp;
pub mod oauth;
pub mod encode;
pub mod sql;
pub mod stats;
pub mod hearthstone;
pub mod blob;
pub mod squad;
pub mod riot;
pub mod job;
pub mod session;
pub mod wow;
pub mod kafka;
pub mod analytics;
pub mod rabbitmq;
pub mod matches;
pub mod games;
pub mod email;
pub mod aimlab;
pub mod access;
pub mod encrypt;
pub mod http;
pub mod csgo;
pub mod math;
pub mod steam;
pub mod share;
pub mod community;
pub mod subscriptions;
pub mod user;
pub mod accounts;
pub mod twitch;
pub mod storage;
pub mod aws;
pub mod hardware;
pub mod ipstack;
pub mod segment;
pub mod profile;
pub mod image;
pub mod config;
pub mod discord;
pub mod redis;
pub mod zendesk;
pub mod ff14;
pub mod combatlog;

pub use error::*;
pub use parse::*;
pub use hal::*;
pub use vod::*;
pub use speed_check::*;
pub use gcp::*;
pub use oauth::*;
pub use encode::*;
pub use sql::*;
pub use blob::*;
pub use squad::*;
pub use riot::*;
pub use job::*;
pub use session::*;
pub use wow::*;
pub use kafka::*;
pub use games::*;
pub use email::*;
pub use aimlab::*;

pub mod proto {
    pub mod hearthstone {
        include!(concat!(env!("OUT_DIR"), "/squadov.hearthstone.game_state.rs"));
    }

    pub mod csgo {
        include!(concat!(env!("OUT_DIR"), "/protobuf.csgo.rs"));
    }
}