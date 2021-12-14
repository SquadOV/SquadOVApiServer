pub mod api;
pub mod db;
pub mod games;
pub mod rso;

use crate::SquadOvError;
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
    pub agent_povs: Option<Vec<String>>,
    pub is_winner: Option<bool>,
    pub rank_low: Option<i32>,
    pub rank_high: Option<i32>,
    pub pov_events: Option<Vec<games::valorant::ValorantMatchFilterEvents>>,
    pub friendly_composition: Option<Vec<Vec<String>>>,
    pub enemy_composition: Option<Vec<Vec<String>>>,
}

impl ValorantMatchFilters {
    pub fn build_friendly_composition_filter(&self) -> Result<String, SquadOvError> {
        ValorantMatchFilters::build_composition_filter(self.friendly_composition.as_ref())
    }

    pub fn build_enemy_composition_filter(&self) -> Result<String, SquadOvError> {
        ValorantMatchFilters::build_composition_filter(self.enemy_composition.as_ref())
    }

    fn build_composition_filter(f: Option<&Vec<Vec<String>>>) -> Result<String, SquadOvError> {
        Ok(
            if let Some(inner) = f {
                let mut pieces: Vec<String> = vec![];
                for x in inner {
                    // It could be empty in which case we want to match anything.
                    if x.is_empty() {
                        continue;
                    }

                    // Each JSON array needs to be converted into a regex lookahead group
                    // that looks like: (?=.*,(1|2|3),)
                    pieces.push(format!(
                        "(?=.*,({}),)",
                        x.into_iter().map(|y| {
                            y.clone().to_lowercase()
                        })
                            .collect::<Vec<String>>()
                            .join("|")
                    ));
                }
                format!("^{}.*$", pieces.join(""))
            } else {
                String::from(".*")
            }
        )
    }
}