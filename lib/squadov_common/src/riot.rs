pub mod api;
pub mod db;
pub mod games;

use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize)]
pub struct RiotAccount {
    pub puuid: String,
    #[serde(rename="gameName")]
    pub game_name: Option<String>,
    #[serde(rename="tagLine")]
    pub tag_line: Option<String>
}

#[derive(Serialize, Deserialize)]
pub struct RiotSummoner {
    pub puuid: String,
    #[serde(rename="accountId")]
    pub account_id: Option<String>,
    #[serde(rename="summonerId")]
    pub summoner_id: Option<String>,
    #[serde(rename="summonerName")]
    pub summoner_name: Option<String>,
    #[serde(rename="lastBackfillLolTime")]
    pub last_backfill_lol_time: Option<DateTime<Utc>>,
    #[serde(rename="lastBackfillTftTime")]
    pub last_backfill_tft_time: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct RiotSummonerDto {
    #[serde(rename="accountId")]
    pub account_id: String,
    pub name: String,
    pub id: String,
    pub puuid: String
}