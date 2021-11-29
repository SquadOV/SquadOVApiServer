pub mod oauth;
pub mod api;
pub mod db;

use serde::{Serialize, Deserialize};

#[derive(Deserialize,Debug,Clone)]
pub struct DiscordConfig {
    pub base_url: String,
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize,Deserialize,Debug,Clone)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar: Option<String>,
}