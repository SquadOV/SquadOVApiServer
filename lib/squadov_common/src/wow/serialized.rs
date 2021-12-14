use serde::Serialize;
use chrono::{DateTime, Utc};
use super::{
    WoWSpellAuraType
};

#[derive(Serialize)]
pub struct SerializedWoWResurrection {
    pub guid: String,
    pub name: String,
    pub flags: i64,
    pub tm: DateTime<Utc>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SerializedWoWDeath {
    pub event_id: i64,
    pub guid: String,
    pub name: String,
    pub flags: i64,
    pub tm: DateTime<Utc>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SerializedWoWAura {
    pub target_guid: String,
    pub target_name: String,
    pub spell_id: i64,
    pub aura_type: WoWSpellAuraType,
    pub applied_tm: DateTime<Utc>,
    pub removed_tm: DateTime<Utc>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SerializedWowEncounter {
    pub encounter_name: String,
    pub start_tm: DateTime<Utc>,
    pub end_tm: DateTime<Utc>
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SerializedWoWSpellCast {
    pub source_guid: String,
    pub source_name: String,
    pub source_flags: i64,
    pub target_guid: Option<String>,
    pub target_name: Option<String>,
    pub target_flags: Option<i64>,
    pub cast_start: Option<DateTime<Utc>>,
    pub cast_finish: DateTime<Utc>,
    pub spell_id: i64,
    pub spell_school: i32,
    pub success: bool,
    pub instant: bool,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct SerializedWoWAuraBreak {
    pub source_guid: String,
    pub source_name: String,
    pub source_flags: i64,
    pub target_guid: String,
    pub target_name: String,
    pub target_flags: i64,
    pub aura_id: i64,
    pub aura_type: WoWSpellAuraType,
    pub spell_id: Option<i64>,
    pub tm: DateTime<Utc>,
}