use actix_web::{HttpResponse, Result, web};
use serde::{Serialize, Deserialize, Deserializer};
use chrono::{DateTime, Utc};
use std::{
    sync::Arc,
    collections::HashMap,
};
use uuid::Uuid;
use crate::shared::SharedApp;

#[derive(PartialEq)]
pub enum WowInstanceMode {
    Arena,
    Unknown
}

impl<'de> Deserialize<'de> for WowInstanceMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.to_lowercase().as_str() {
            "arena" => WowInstanceMode::Arena,
            _ => WowInstanceMode::Unknown,
        })
    }
}

#[derive(PartialEq)]
pub enum WowRelease {
    Retail,
    Tbc,
    Vanilla,
    Unknown
}

impl<'de> Deserialize<'de> for WowRelease {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.to_lowercase().as_str() {
            "retail" => WowRelease::Retail,
            "tbc" => WowRelease::Tbc,
            "vanilla" => WowRelease::Vanilla,
            _ => WowRelease::Unknown,
        })
    }
}

impl WowRelease {
    fn to_patch_filter(&self) -> String {
        match self {
            WowRelease::Retail => "9.*",
            WowRelease::Tbc => "2.*",
            WowRelease::Vanilla => "1.*",
            _ => ".*",
        }.to_string()
    }
}

#[derive(PartialEq)]
pub enum WowArenaBracket {
    Skrimish,
    Arena5v5,
    RatedBg,
    Arena3v3,
    Arena2v2,
    Unknown
}

impl<'de> Deserialize<'de> for WowArenaBracket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.to_lowercase().as_str() {
            "skirmish" => WowArenaBracket::Skrimish,
            "5v5" => WowArenaBracket::Arena5v5,
            "rated bg" => WowArenaBracket::RatedBg,
            "3v3" => WowArenaBracket::Arena3v3,
            "2v2" => WowArenaBracket::Arena2v2,
            _ => WowArenaBracket::Unknown,
        })
    }
}

impl ToString for WowArenaBracket {
    fn to_string(&self) -> String {
        match self {
            WowArenaBracket::Skrimish => "Skirmish",
            WowArenaBracket::Arena5v5 => "5v5",
            WowArenaBracket::RatedBg => "Rated BG",
            WowArenaBracket::Arena3v3 => "3v3",
            WowArenaBracket::Arena2v2 => "2v2",
            WowArenaBracket::Unknown => "Unknown",
        }.to_string()
    }
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct RawWowRequest {
    start_tm: DateTime<Utc>,
    end_tm: DateTime<Utc>,
    mode: WowInstanceMode,
    release: WowRelease,
    patch: Option<String>,
    bracket: Option<WowArenaBracket>,
}

#[derive(Serialize)]
pub struct WowCombatantInfo {
    player_guid: String,
    spec_id: i32,
    class_id: Option<i32>,
    rating: i32,
    team: i32,
    items: serde_json::Value,
    talents: serde_json::Value,
    covenant: serde_json::Value,
}

#[derive(Serialize)]
pub struct RawWowResponse {
    id: Uuid,
    tm: chrono::NaiveDateTime,
    build: String,
    // Keep it in json format since we don't really need to process it on the server.
    info: serde_json::Value,
    combatants: Vec<WowCombatantInfo>,
}

pub async fn raw_wow_handler(payload: web::Json<RawWowRequest>, app: web::Data<Arc<SharedApp>>) -> Result<HttpResponse> {
    if payload.mode == WowInstanceMode::Unknown ||
        payload.release == WowRelease::Unknown
    {
        return Ok(HttpResponse::BadRequest().finish())
    }

    if payload.end_tm.signed_duration_since(payload.start_tm).num_seconds() > 86400 {
        return Ok(HttpResponse::BadRequest().finish())
    }

    let client = app.redshift.get().await.unwrap();
    
    let mut resp: HashMap<String, RawWowResponse> = HashMap::new();
    
    match payload.mode {
        WowInstanceMode::Arena => {
            // Slightly inefficient since we return 1 row per combatant rather than 1 row per match.
            // Unfortunately, there's no good way to aggregate all the combatant data into a single "combatants" column.
            // Thus, it'd be better to just have a single trip to the Redshift cluster and get all the info all at once.
            let stmt = client.prepare_cached(r#"
                SELECT
                    wm.id,
                    wm.tm,
                    wm.build,
                    JSON_SERIALIZE(wm.info),
                    wmc.player_guid,
                    wmc.spec_id,
                    wmc.class_id,
                    wmc.rating,
                    wmc.team,
                    JSON_SERIALIZE(wmc.items),
                    JSON_SERIALIZE(wmc.talents),
                    JSON_SERIALIZE(wmc.covenant)
                FROM wow_matches wm
                LEFT JOIN wow_match_combatants wmc
                    ON wmc.match_id = wm.id
                WHERE wm.tm >= $1 AND wm.tm < $2
                    AND wm.match_type = 'arena'
                    AND (wm.build ~ $3 OR wm.build ~ $4)
                    AND wm.info.arena_type::VARCHAR = $5
            "#).await.unwrap();

            let rows = client.query(&stmt, &[
                &payload.start_tm.naive_utc(),
                &payload.end_tm.naive_utc(),
                &payload.release.to_patch_filter(),
                &payload.patch.clone().unwrap_or(".*".to_string()),
                &payload.bracket.as_ref().unwrap().to_string(),
            ]).await.unwrap();

            for x in rows {
                let id: String = x.get(0);

                if !resp.contains_key(&id) {
                    let tm: chrono::NaiveDateTime = x.get(1);
                    let build: String = x.get(2);
                    let info: String = x.get(3);
                    resp.insert(id.clone(), RawWowResponse{
                        id: id.parse().unwrap(),
                        tm,
                        build,
                        info: serde_json::from_str(&info).unwrap(),
                        combatants: vec![],
                    });
                }

                if let Some(data) = resp.get_mut(&id) {
                    let player_guid: Option<String> = x.get(4);
                    if player_guid.is_none() {
                        continue;
                    }
                    let player_guid = player_guid.unwrap();

                    let spec_id: i32 = x.get(5);
                    let class_id: Option<i32> = x.get(6);
                    let rating: i32 = x.get(7);
                    let team: i32 = x.get(8);
                    let items: String = x.get(9);
                    let talents: String = x.get(10);
                    let covenant: String = x.get(11);
                    data.combatants.push(WowCombatantInfo{
                        player_guid,
                        spec_id,
                        class_id,
                        rating,
                        team,
                        items: serde_json::from_str(&items).unwrap(),
                        talents: serde_json::from_str(&talents).unwrap(),
                        covenant: serde_json::from_str(&covenant).unwrap(),
                    });
                }
            }
        },
        _ => return Ok(HttpResponse::BadRequest().finish()) 
    };

    Ok(
        HttpResponse::Ok().json(resp.values().collect::<Vec<_>>())
    )
}