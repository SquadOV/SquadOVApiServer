pub mod api;
pub mod db;
pub mod games;
pub mod rso;

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

#[derive(Deserialize)]
pub struct RiotUserInfo {
    pub sub: String,
    pub cpid: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct LolMatchFilters {
    maps: Option<Vec<i32>>,
    modes: Option<Vec<String>>,
    has_vod: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct TftMatchFilters {
    has_vod: Option<bool>,
}

#[derive(Deserialize, Debug, Default)]
#[serde(rename_all="camelCase")]
pub struct ValorantMatchFilters {
    pub maps: Option<Vec<String>>,
    pub modes: Option<Vec<String>>,
    pub has_vod: Option<bool>,
    pub is_ranked: Option<bool>,
}