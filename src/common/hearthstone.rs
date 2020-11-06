use serde::{Serialize,Deserialize};
use chrono::{DateTime, Utc, serde::ts_seconds};
use ipnetwork::IpNetwork;

#[derive(Deserialize)]
pub struct HearthstoneGameConnectionInfo {
    pub ip: IpNetwork,
    pub port: i32,
    #[serde(rename = "gameId")]
    pub game_id: i32,
    #[serde(rename = "clientId")]
    pub client_id: i32,
    #[serde(rename = "spectateKey")]
    pub spectate_key: String,
    pub reconnecting: bool
}

#[derive(Deserialize)]
pub struct HearthstoneCardCount {
    pub normal: i32,
    pub golden: i32
}

#[derive(Deserialize)]
pub struct HearthstoneDeckSlot {
    pub index: i32,
    #[serde(rename = "cardId")]
    pub card_id: String,
    pub owned: bool,
    pub count: HearthstoneCardCount
}

#[derive(Deserialize)]
pub struct HearthstoneDeck {
    pub name: String,
    #[serde(rename = "deckId")]
    pub deck_id: i64,
    #[serde(rename = "heroCard")]
    pub hero_card: String,
    #[serde(rename = "heroPremium")]
    pub hero_premium: i32,
    #[serde(rename = "deckType")]
    pub deck_type: i32,
    #[serde(rename = "createDate", with="ts_seconds")]
    pub create_date: DateTime<Utc>,
    #[serde(rename = "isWild")]
    pub is_wild: bool,
    pub slots: Vec<HearthstoneDeckSlot>
}

#[derive(Deserialize)]
pub struct HearthstoneMedalInfo {
    #[serde(rename = "leagueId")]
    pub league_id: i32,
    #[serde(rename = "earnedStars")]
    pub earned_stars: i32,
    #[serde(rename = "starLevel")]
    pub star_level: i32,
    #[serde(rename = "bestStarLevel")]
    pub best_star_level: i32,
    #[serde(rename = "winStreak")]
    pub win_streak: i32,
    #[serde(rename = "legendIndex")]
    pub legend_index: i32
}

#[derive(Deserialize)]
pub struct HearthstonePlayerMedalInfo {
    pub standard: HearthstoneMedalInfo,
    pub wild: HearthstoneMedalInfo
}

#[derive(Deserialize)]
pub struct HearthstonePlayer {
    pub name: String,
    pub local: bool,
    pub side: i32,
    #[serde(rename = "cardBackId")]
    pub card_back_id: i32,
    #[serde(rename = "medalInfo")]
    pub medal_info: HearthstonePlayerMedalInfo,
    #[serde(rename = "arenaWins")]
    pub arena_wins: u32,
    #[serde(rename = "arenaLoss")]
    pub arena_loss: u32,
    #[serde(rename = "tavernBrawlWins")]
    pub tavern_brawl_wins: u32,
    #[serde(rename = "tavernBrawlLoss")]
    pub tavern_brawl_loss: u32
}

#[derive(Serialize,Deserialize)]
pub struct HearthstoneRawLog {
    pub time: DateTime<Utc>,
    pub section: String,
    pub log: String
}