use crate::SquadOvError;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::{Executor, Postgres};
use super::WoWCombatLogState;

#[derive(Deserialize)]
pub struct WoWEncounterStart {
    #[serde(rename="encounterId")]
    pub encounter_id: i32,
    #[serde(rename="encounterName")]
    pub encounter_name: String,
    pub difficulty: i32,
    #[serde(rename="numPlayers")]
    pub num_players: i32,
    #[serde(rename="instanceId")]
    pub instance_id: i32
}

#[derive(Deserialize)]
pub struct WoWEncounterEnd {
    #[serde(rename="encounterId")]
    pub encounter_id: i32,
    #[serde(rename="encounterName")]
    pub encounter_name: String,
    pub difficulty: i32,
    #[serde(rename="numPlayers")]
    pub num_players: i32,
    pub success: bool
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WoWEncounter {
    #[serde(rename="matchUuid")]
    pub match_uuid: Uuid,
    pub tm: DateTime<Utc>,
    #[serde(rename="combatantsKey")]
    pub combatants_key: String,
    #[serde(rename="encounterId")]
    pub encounter_id: i32,
    #[serde(rename="encounterName")]
    pub encounter_name: String,
    pub difficulty: i32,
    #[serde(rename="numPlayers")]
    pub num_players: i32,
    #[serde(rename="instanceId")]
    pub instance_id: i32,
    #[serde(rename="finishTime")]
    pub finish_time: Option<DateTime<Utc>>,
    pub success: bool,
    #[serde(rename="userUuid")]
    pub user_uuid: Uuid,
    pub build: String
}

#[derive(Deserialize)]
pub struct WoWChallengeStart {
    #[serde(rename="challengeName")]
    pub challenge_name: String,
    #[serde(rename="instanceId")]
    pub instance_id: i32,
    #[serde(rename="keystoneLevel")]
    pub keystone_level: i32,
}

#[derive(Deserialize)]
pub struct WoWChallengeEnd {
    #[serde(rename="instanceId")]
    pub instance_id: i32,
    #[serde(rename="keystoneLevel")]
    pub keystone_level: i32,
    pub success: bool,
    #[serde(rename="timeMs")]
    pub time_ms: i64
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WoWChallenge {
    #[serde(rename="matchUuid")]
    pub match_uuid: Uuid,
    pub tm: DateTime<Utc>,
    #[serde(rename="combatantsKey")]
    pub combatants_key: String,
    #[serde(rename="challengeName")]
    pub challenge_name: String,
    #[serde(rename="instanceId")]
    pub instance_id: i32,
    #[serde(rename="keystoneLevel")]
    pub keystone_level: i32,
    #[serde(rename="finishTime")]
    pub finish_time: Option<DateTime<Utc>>,
    #[serde(rename="timeMs")]
    pub time_ms: i64,
    pub success: bool,
    #[serde(rename="userUuid")]
    pub user_uuid: Uuid,
    pub build: String
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WoWArenaStart {
    pub instance_id: i32,
    #[serde(rename="type")]
    pub arena_type: String,
    pub local_team_id: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WoWArenaEnd {
    pub winning_team_id: i32,
    pub match_duration_seconds: i32,
    pub new_ratings: Vec<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WoWArena {
    pub match_uuid: Uuid,
    pub tm: DateTime<Utc>,
    pub combatants_key: String,
    pub instance_id: i32,
    #[serde(rename="type")]
    pub arena_type: String,
    pub finish_time: Option<DateTime<Utc>>,
    pub winning_team_id: Option<i32>,
    pub match_duration_seconds: Option<i32>,
    pub new_ratings: Option<Vec<i32>>,
    pub user_uuid: Uuid,
    pub success: bool,
    pub build: String
}

#[derive(Clone)]
pub struct GenericWoWMatchView {
    pub alt_id: i64,
    pub user_id: i64,
    combat_log_version: String,
    advanced_log: bool,
    build_version: String,
}

pub async fn get_generic_wow_match_view_from_id<'a, T>(ex: T, id: &Uuid) -> Result<GenericWoWMatchView, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            GenericWoWMatchView,
            "
            SELECT alt_id, user_id, combat_log_version, advanced_log, build_version
            FROM squadov.wow_match_view
            WHERE id = $1
            ",
            id,
        )
            .fetch_one(ex)
            .await?
    )
}

impl GenericWoWMatchView {
    pub fn combat_log_state(&self) -> WoWCombatLogState {
        WoWCombatLogState {
            combat_log_version: self.combat_log_version.clone(),
            advanced_log: self.advanced_log,
            build_version: self.build_version.clone(),
        }
    }
}