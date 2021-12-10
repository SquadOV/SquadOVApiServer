use serde::{Serialize,Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde_repr::{Serialize_repr, Deserialize_repr};
use num_enum::TryFromPrimitive;

#[derive(Copy, Clone, Serialize_repr, Deserialize_repr, Debug, TryFromPrimitive, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ValorantMatchFilterEvents {
    DoubleKill,
    TripleKill,
    QuadraKill,
    PentaKill,
}

#[derive(Deserialize)]
pub struct ValorantMatchlistDto {
    pub history: Vec<ValorantMatchlistEntryDto>
}

#[derive(Deserialize)]
pub struct ValorantMatchlistEntryDto {
    #[serde(rename="matchId")]
    pub match_id: String
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchDto {
    #[serde(rename="matchInfo")]
    pub match_info: ValorantMatchInfoDto,
    pub players: Vec<ValorantMatchPlayerDto>,
    pub teams: Vec<ValorantMatchTeamDto>,
    #[serde(rename="roundResults")]
    pub round_results: Vec<ValorantMatchRoundResultDto>,
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchInfoDto {
    #[serde(rename="matchId")]
    pub match_id: String,
    #[serde(rename="mapId")]
    pub map_id: Option<String>,
    #[serde(rename="gameLengthMillis")]
    pub game_length_millis: i32,
    #[serde(rename(serialize="serverStartTimeUtc", deserialize="gameStartMillis"), deserialize_with="crate::parse_utc_time_from_milliseconds")]
    pub server_start_time_utc: Option<DateTime<Utc>>,
    #[serde(rename="provisioningFlowId")]
    pub provisioning_flow_id: Option<String>,
    #[serde(rename="gameMode")]
    pub game_mode: Option<String>,
    #[serde(rename="isRanked")]
    pub is_ranked: Option<bool>,
    #[serde(rename="seasonId")]
    pub season_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchPlayerDto {
    pub puuid: String,
    #[serde(rename="teamId")]
    pub team_id: String,
    #[serde(rename="characterId")]
    pub character_id: Option<String>,
    pub stats: Option<ValorantMatchPlayerStatsDto>,
    #[serde(rename="competitiveTier", default)]
    pub competitive_tier: i32,
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchPlayerStatsDto {
    pub score: i32,
    #[serde(rename="roundsPlayed")]
    pub rounds_played: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchTeamDto {
    #[serde(rename="teamId")]
    pub team_id: String,
    pub won: bool,
    #[serde(rename="roundsPlayed")]
    pub rounds_played: i32,
    #[serde(rename="roundsWon")]
    pub rounds_won: i32,
    #[serde(rename="numPoints")]
    pub num_points: i32
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchRoundResultDto {
    #[serde(rename="roundNum")]
    pub round_num: i32,
    #[serde(rename="winningTeam")]
    pub winning_team: String,
    #[serde(rename="bombPlanter")]
    pub bomb_planter: Option<String>,
    #[serde(rename="bombDefuser")]
    pub bomb_defuser: Option<String>,
    #[serde(rename="plantRoundTime")]
    pub plant_round_time: Option<i32>,
    #[serde(rename="defuseRoundTime")]
    pub defuse_round_time: Option<i32>,
    #[serde(rename="playerStats")]
    pub player_stats: Vec<ValorantMatchPlayerRoundStatsDto>,
}

impl ValorantMatchRoundResultDto {
    pub fn flatten(&self, round_num: i32) -> (Vec<FlatValorantMatchPlayerRoundStatsDto>, Vec<FlatValorantMatchKillDto>, Vec<FlatValorantMatchDamageDto>, Vec<FlatValorantMatchEconomyDto>) {
        let stats: Vec<FlatValorantMatchPlayerRoundStatsDto> = self.player_stats.iter().map(|x| {
            FlatValorantMatchPlayerRoundStatsDto {
                puuid: x.puuid.clone(),
                score: x.score,
                round_num,
            }
        }).collect();

        let kills: Vec<FlatValorantMatchKillDto> = self.player_stats.iter().map(|x| {
            x.kills.iter().map(|y| {
                FlatValorantMatchKillDto{
                    round_num,
                    base: y.clone(),
                }
            }).collect::<Vec<FlatValorantMatchKillDto>>()
        }).flatten().collect();

        let damage: Vec<FlatValorantMatchDamageDto> = self.player_stats.iter().map(|x| {
            x.damage.iter().map(|y| {
                FlatValorantMatchDamageDto{
                    round_num,
                    instigator: x.puuid.clone(),
                    base: y.clone(),
                }
            }).collect::<Vec<FlatValorantMatchDamageDto>>()
        }).flatten().collect();

        let econ: Vec<FlatValorantMatchEconomyDto> = self.player_stats.iter().map(|x| {
            FlatValorantMatchEconomyDto {
                round_num,
                puuid: x.puuid.clone(),
                base: x.economy.clone(),
            }
        }).collect();

        (stats, kills, damage, econ)
    }
}

pub struct FlatValorantMatchPlayerRoundStatsDto {
    pub puuid: String,
    pub score: i32,
    pub round_num: i32,
}

pub struct FlatValorantMatchKillDto {
    pub round_num: i32,
    pub base: ValorantMatchKillDto
}

#[derive(Clone)]
pub struct FlatValorantMatchDamageDto {
    pub round_num: i32,
    pub instigator: String,
    pub base: ValorantMatchDamageDto 
}

pub struct FlatValorantMatchEconomyDto {
    pub round_num: i32,
    pub puuid: String,
    pub base: ValorantMatchEconomyDto 
}

#[derive(Serialize, Deserialize)]
pub struct ValorantMatchPlayerRoundStatsDto {
    pub puuid: String,
    pub kills: Vec<ValorantMatchKillDto>,
    pub damage: Vec<ValorantMatchDamageDto>,
    pub economy: ValorantMatchEconomyDto,
    pub score: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValorantMatchKillDto {
    #[serde(rename="timeSinceGameStartMillis")]
    pub time_since_game_start_millis: i32,
    #[serde(rename="timeSinceRoundStartMillis")]
    pub time_since_round_start_millis: i32,
    pub killer: Option<String>,
    pub victim: String,
    pub assistants: Vec<String>,
    #[serde(rename="finishingDamage")]
    pub finishing_damage: ValorantMatchFinishingDamageDto
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValorantMatchFinishingDamageDto {
    #[serde(rename="damageType")]
    pub damage_type: String,
    #[serde(rename="damageItem")]
    pub damage_item: String,
    #[serde(rename="isSecondaryFireMode")]
    pub is_secondary_fire_mode: bool
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValorantMatchDamageDto {
    pub receiver: String,
    pub damage: i32,
    pub legshots: i32,
    pub bodyshots: i32,
    pub headshots: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ValorantMatchEconomyDto {
    #[serde(rename="loadoutValue")]
    pub loadout_value: i32,
    pub weapon: String,
    pub armor: String,
    pub remaining: i32,
    pub spent: i32,
}

#[derive(Serialize,Deserialize, Clone, Debug)]
pub struct ValorantPlayerMatchSummary {
    #[serde(rename = "matchId")]
    pub match_id: String,
    #[serde(rename = "matchUuid")]
    pub match_uuid: Uuid,
    #[serde(rename = "serverStartTimeUtc")]
    pub server_start_time_utc: Option<DateTime<Utc>>,
    #[serde(rename = "gameMode")]
    pub game_mode: Option<String>,
    #[serde(rename = "mapId")]
    pub map_id: Option<String>,
    #[serde(rename = "isRanked")]
    pub is_ranked: Option<bool>,
    #[serde(rename = "provisioningFlowId")]
    pub provisioning_flow_id: Option<String>,
    #[serde(rename = "characterId")]
    pub character_id: String,
    pub won: bool,
    #[serde(rename = "roundsWon")]
    pub rounds_won: i32,
    #[serde(rename = "roundsLost")]
    pub rounds_lost: i32,
    #[serde(rename = "combatScoreRank")]
    pub combat_score_rank: i64,
    #[serde(rename = "competitiveTier")]
    pub competitive_tier: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    #[serde(rename = "roundsPlayed")]
    pub rounds_played: i32,
    #[serde(rename = "totalCombatScore")]
    pub total_combat_score: i32,
    #[serde(rename = "totalDamage")]
    pub total_damage: i64,
    pub headshots: i64,
    pub bodyshots: i64,
    pub legshots: i64,
    #[serde(rename = "userUuid")]
    pub user_uuid: Uuid,
    pub puuid: String,
}
