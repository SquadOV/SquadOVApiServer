use actix_web::{web, HttpResponse};
use crate::api;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    SerializedWoWDeath,
    SerializedWoWAura,
    SerializedWowEncounter,
    SerializedWoWResurrection,
    SerializedWoWSpellCast,
    SerializedWoWAuraBreak,
    WoWSpellAuraType,
    WowDeathRecap,
    WowDeathRecapEvent,
};
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use std::str::FromStr;

impl api::ApiApplication {
    async fn get_wow_death_recap(&self, view_uuid: &Uuid, event_id: i64, seconds: i32) -> Result<WowDeathRecap, SquadOvError> {
        let death_window = sqlx::query!(
            r#"
            SELECT 
                wve.tm - $2::INTEGER * INTERVAL '1 second' AS "start!",
                wve.tm AS "end!",
                wve.dest_char AS "dest_char?"
            FROM squadov.wow_match_view_death_events AS wde
            INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.event_id = wde.event_id
            WHERE wde.event_id = $1
            "#,
            event_id,
            seconds
        )
            .fetch_one(&*self.pool)
            .await?;

        if death_window.dest_char.is_none() {
            return Err(SquadOvError::BadRequest);
        }

        let dest_char = death_window.dest_char.unwrap();
        Ok(WowDeathRecap{
            hp_events: sqlx::query_as!(
                WowDeathRecapEvent,
                r#"
                SELECT
                    data.tm AS "tm!",
                    (EXTRACT(EPOCH FROM data.tm - $3) * 1000)::BIGINT AS "diff_ms!",
                    (data.amount)::INTEGER AS "diff_hp!",
                    data.spell_id AS "spell_id?",
                    data.source_guid AS "source_guid?",
                    data.source_name AS "source_name?"
                FROM (
                    SELECT
                        wve.tm,
                        wve.log_line,
                        0-wde.amount AS "amount",
                        wde.spell_id,
                        wcp.unit_name AS "source_name",
                        wcp.unit_guid AS "source_guid"
                    FROM squadov.wow_match_view_damage_events AS wde
                    INNER JOIN squadov.wow_match_view_events AS wve
                        ON wve.event_id = wde.event_id
                    LEFT JOIN squadov.wow_match_view_character_presence AS wcp
                        ON wcp.character_id = wve.source_char
                    INNER JOIN squadov.wow_match_view AS wmv
                        ON wmv.alt_id = wve.view_id
                    WHERE wmv.id = $1
                        AND wve.tm BETWEEN $2 AND $3
                        AND wve.dest_char = $4
                    UNION
                    SELECT
                        wve.tm,
                        wve.log_line,
                        whe.amount,
                        whe.spell_id,
                        wcp.unit_name AS "source_name",
                        wcp.unit_guid AS "source_guid"
                    FROM squadov.wow_match_view_healing_events AS whe
                    INNER JOIN squadov.wow_match_view_events AS wve
                        ON wve.event_id = whe.event_id
                    LEFT JOIN squadov.wow_match_view_character_presence AS wcp
                        ON wcp.character_id = wve.source_char
                    INNER JOIN squadov.wow_match_view AS wmv
                        ON wmv.alt_id = wve.view_id
                    WHERE wmv.id = $1
                        AND wve.tm BETWEEN $2 AND $3
                        AND wve.dest_char = $4
                ) AS data
                ORDER BY data.log_line DESC
                "#,
                view_uuid,
                death_window.start,
                death_window.end,
                dest_char,
            )
                .fetch_all(&*self.pool)
                .await?
        })
    }

    async fn get_wow_match_subencounters(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWowEncounter>, SquadOvError> {
        let subencounter_events = sqlx::query!(
            r#"
            SELECT
                wve.tm,
                see.encounter_name,
                see.is_start
            FROM squadov.wow_match_view_subencounter_events AS see
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.event_id = see.event_id
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.alt_id = wve.view_id
            WHERE wmv.id = $1
            ORDER BY wve.tm ASC
            "#,
            view_uuid
        )
            .fetch_all(&*self.heavy_pool)
            .await?;

        let raw_starts: Vec<_> = subencounter_events.iter().filter(|x| {
            x.is_start
        }).collect();

        let raw_ends: Vec<_> = subencounter_events.iter().filter(|x| {
            !x.is_start
        }).collect();

        // For each encounter name, we get a list of when that encounter ends.
        // Note that an encounter can appear multiple times (theoretically but probably not practically).
        let mut encounter_end_hashmap: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
        for end in &raw_ends {
            if !encounter_end_hashmap.contains_key(&end.encounter_name) {
                encounter_end_hashmap.insert(end.encounter_name.clone(), vec![]);
            }

            let inner_vec = encounter_end_hashmap.get_mut(&end.encounter_name).unwrap();
            inner_vec.push(end.tm.clone());
        }

        Ok(
            raw_starts.into_iter()
                .map(|x| {
                    let mut end_tm: DateTime<Utc> = Utc::now();
                    if encounter_end_hashmap.contains_key(&x.encounter_name) {
                        let inner_vec = encounter_end_hashmap.get(&x.encounter_name).unwrap();
                        let idx = match inner_vec.binary_search(&x.tm) {
                            Ok(x) => x,
                            Err(x) => x
                        };

                        if idx < inner_vec.len() {
                            end_tm = inner_vec[idx];
                        }
                    }

                    SerializedWowEncounter{
                        encounter_name: x.encounter_name.clone(),
                        start_tm: x.tm.clone(),
                        end_tm,
                    }
                })
                .collect()
        )
    }

    async fn get_wow_match_aura_events(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWoWAura>, SquadOvError> {
        let raw_auras = sqlx::query!(
            r#"
            SELECT
                dest.unit_guid AS "target_guid",
                COALESCE(dest.unit_name, dest.unit_guid) AS "target_name!",
                wae.spell_id,
                wae.aura_type,
                wae.applied,
                wve.tm
            FROM squadov.wow_match_view_aura_events AS wae
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.event_id = wae.event_id
            INNER JOIN squadov.wow_match_view_character_presence AS dest
                ON dest.character_id = wve.dest_char
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.alt_id = wve.view_id
            WHERE wmv.id = $1
            ORDER BY wve.tm ASC
            "#,
            view_uuid,
        )
            .fetch_all(&*self.heavy_pool)
            .await?;

        // We need to match the end of an aura with the start of an aura. We do this based on spell id and the target
        // as we assume that the player will only have one instance of an aura running at any time.
        let mut removed_aura_hashmap: HashMap<String, HashMap<i64, Vec<DateTime<Utc>>>> = HashMap::new();
        for aura in &raw_auras {
            if aura.applied {
                continue;
            }

            if !removed_aura_hashmap.contains_key(&aura.target_guid) {
                removed_aura_hashmap.insert(aura.target_guid.clone(), HashMap::new());
            }

            let spell_id_hashmap = removed_aura_hashmap.get_mut(&aura.target_guid).unwrap();
            if !spell_id_hashmap.contains_key(&aura.spell_id) {
                spell_id_hashmap.insert(aura.spell_id, vec![]);
            }

            let removed_tms = spell_id_hashmap.get_mut(&aura.spell_id).unwrap();
            removed_tms.push(aura.tm);
        }

        Ok(
            raw_auras.into_iter()
                .filter(|x| {
                    x.applied
                })
                .map(|x| {
                    let mut removed_tm: DateTime<Utc> = Utc::now();
                    if removed_aura_hashmap.contains_key(&x.target_guid) {
                        let spell_id_hashmap = removed_aura_hashmap.get(&x.target_guid).unwrap();
                        if spell_id_hashmap.contains_key(&x.spell_id) {
                            let inner_vec = spell_id_hashmap.get(&x.spell_id).unwrap();
                            let idx = match inner_vec.binary_search(&x.tm) {
                                Ok(x) => x,
                                Err(x) => x
                            };

                            if idx < inner_vec.len() {
                                removed_tm = inner_vec[idx];
                            }
                        }
                    }

                    SerializedWoWAura{
                        target_guid: x.target_guid,
                        target_name: x.target_name,
                        spell_id: x.spell_id,
                        aura_type: WoWSpellAuraType::from_str(&x.aura_type).map_or(WoWSpellAuraType::Unknown, |x| { x }),
                        applied_tm: x.tm,
                        removed_tm,
                    }
                })
                .collect()
        )
    }

    async fn get_wow_match_death_events(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWoWDeath>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                SerializedWoWDeath,
                r#"
                SELECT
                    wde.event_id,
                    dest.unit_guid AS "guid",
                    COALESCE(dest.unit_name, dest.unit_guid) AS "name!",
                    COALESCE(wve.dest_flags, dest.flags) AS "flags!",
                    wve.tm
                FROM squadov.wow_match_view_death_events AS wde
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.event_id = wde.event_id
                INNER JOIN squadov.wow_match_view_character_presence AS dest
                    ON dest.character_id = wve.dest_char
                INNER JOIN squadov.wow_match_view AS wmv
                    ON wmv.alt_id = wve.view_id
                WHERE wmv.id = $1
                ORDER BY wve.tm ASC
                "#,
                view_uuid
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

    async fn get_wow_match_resurrection_events(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWoWResurrection>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                SerializedWoWResurrection,
                r#"
                SELECT
                    dest.unit_guid AS "guid",
                    COALESCE(dest.unit_name, dest.unit_guid) AS "name!",
                    COALESCE(wve.dest_flags, dest.flags) AS "flags!",
                    wve.tm
                FROM squadov.wow_match_view_resurrect_events AS wre
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.event_id = wre.event_id
                INNER JOIN squadov.wow_match_view_character_presence AS dest
                    ON dest.character_id = wve.dest_char
                INNER JOIN squadov.wow_match_view AS wmv
                    ON wmv.alt_id = wve.view_id
                WHERE wmv.id = $1
                ORDER BY wve.tm ASC
                "#,
                view_uuid
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

    async fn get_wow_match_aura_break_events(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWoWAuraBreak>, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                SELECT
                    source.unit_guid AS "source_guid",
                    COALESCE(source.unit_name, source.unit_guid) AS "source_name!",
                    COALESCE(wve.source_flags, source.flags) AS "source_flags!",
                    dest.unit_guid AS "target_guid",
                    COALESCE(dest.unit_name, dest.unit_guid) AS "target_name!",
                    COALESCE(wve.dest_flags, dest.flags) AS "target_flags!",
                    wabe.aura_spell_id AS "aura_id!",
                    wabe.aura_type AS "aura_type",
                    wabe.removed_by_spell_id AS "spell_id",
                    wve.tm
                FROM squadov.wow_match_view_aura_break_events AS wabe
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.event_id = wabe.event_id
                INNER JOIN squadov.wow_match_view_character_presence AS source
                    ON source.character_id = wve.source_char
                INNER JOIN squadov.wow_match_view_character_presence AS dest
                    ON dest.character_id = wve.dest_char
                INNER JOIN squadov.wow_match_view AS wmv
                    ON wmv.alt_id = wve.view_id
                WHERE wmv.id = $1
                ORDER BY wve.tm ASC
                "#,
                view_uuid
            )
                .fetch_all(&*self.pool)
                .await?
                .into_iter()
                .map(|x| {
                    SerializedWoWAuraBreak{
                        source_guid: x.source_guid,
                        source_name: x.source_name,
                        source_flags: x.source_flags,
                        target_guid: x.target_guid,
                        target_name: x.target_name,
                        target_flags: x.target_flags,
                        aura_id: x.aura_id,
                        aura_type: WoWSpellAuraType::from_str(&x.aura_type).map_or(WoWSpellAuraType::Unknown, |x| { x }),
                        spell_id: x.spell_id,
                        tm: x.tm,
                    }
                })
                .collect()
        )
    }

    async fn get_wow_match_spell_cast_events(&self, view_uuid: &Uuid) -> Result<Vec<SerializedWoWSpellCast>, SquadOvError> {
        let raw_casts = sqlx::query!(
            r#"
            SELECT
                source.unit_guid AS "source_guid",
                COALESCE(source.unit_name, source.unit_guid) AS "source_name!",
                COALESCE(wve.source_flags, source.flags) AS "source_flags!",
                dest.unit_guid AS "target_guid?",
                COALESCE(dest.unit_name, dest.unit_guid) AS "target_name?",
                COALESCE(wve.dest_flags, dest.flags) AS "target_flags?",
                msce.spell_id,
                msce.spell_school,
                msce.is_start,
                msce.is_finish,
                msce.success,
                wve.tm,
                wve.log_line
            FROM squadov.wow_match_view_spell_cast_events AS msce
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.event_id = msce.event_id
            INNER JOIN squadov.wow_match_view_character_presence AS source
                ON source.character_id = wve.source_char
            LEFT JOIN squadov.wow_match_view_character_presence AS dest
                ON dest.character_id = wve.dest_char
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.alt_id = wve.view_id
            WHERE wmv.id = $1
            ORDER BY wve.log_line ASC
            "#,
            view_uuid
        )
            .fetch_all(&*self.pool)
            .await?;

        // Similar to the aura stuff, we want to match each user's spell casts from its start to its finish (or interrupt).
        // However, note that in the case of spell casts, some spells are instantly cast or fail and don't have a "start".
        let mut raw_start_casts: Vec<_> = vec![];

        // If we didn't wrap the end casts in this structure to properly identify which end casts don't have a corresponding
        // start cast we'd have to remove casts from the Vec<_> once a match has been found. I'm worried that doing too many
        // O(M) shifts will be slow so instead of removing from the vec, we just flag all the casts that do have a match.
        struct CastWrapper<T> {
            used: bool,
            data: T,
        }

        // In contrast to the aura stuff, the spell casts also need a map that keeps track of all the spell casts for that particular spell as well.
        // Why? Because sometimes WoW doesn't feel like printing out spell success/failures! Thus you can get a CAST_START with no matching
        // CAST_SUCCESS/FAILURE. To counteract this, we compare the next spell CAST_START with the same spell ID against the matching CAST_SUCCESS/FAILURE.
        // If the next CAST_START exists and happens before the next CAST_SUCCESS/FAILURE, we know that the first CAST_START failed. We don't know when exactly
        // but oh well.

        let mut raw_start_cast_hashmap: HashMap<String, HashMap<i64, Vec<i64>>> = HashMap::new();
        let mut raw_end_cast_hashmap: HashMap<String, HashMap<i64, Vec<CastWrapper<_>>>> = HashMap::new();

        for c in raw_casts {
            if c.is_start {
                if !raw_start_cast_hashmap.contains_key(&c.source_guid) {
                    raw_start_cast_hashmap.insert(c.source_guid.clone(), HashMap::new());
                }
    
                let spell_id_hashmap = raw_start_cast_hashmap.get_mut(&c.source_guid).unwrap();
                if !spell_id_hashmap.contains_key(&c.spell_id) {
                    spell_id_hashmap.insert(c.spell_id, vec![]);
                }
    
                let raw_casts = spell_id_hashmap.get_mut(&c.spell_id).unwrap();
                raw_casts.push(c.log_line);

                raw_start_casts.push(c);
            } else if c.is_finish {
                if !raw_end_cast_hashmap.contains_key(&c.source_guid) {
                    raw_end_cast_hashmap.insert(c.source_guid.clone(), HashMap::new());
                }
    
                let spell_id_hashmap = raw_end_cast_hashmap.get_mut(&c.source_guid).unwrap();
                if !spell_id_hashmap.contains_key(&c.spell_id) {
                    spell_id_hashmap.insert(c.spell_id, vec![]);
                }
    
                let raw_casts = spell_id_hashmap.get_mut(&c.spell_id).unwrap();
                raw_casts.push(CastWrapper{
                    used: false,
                    data: c,
                });
            }
        }

        // There's 3 classes of casts that we can find:
        //  1) Instant cast - Success
        //  2) Instant cast - Failure
        //  3) Spell with Cast Time
        // Thus, we first go through all the instances where we have a "cast start" and do our best to match
        // them up with the end casts. Once we've done that, we can assume that the rest of the "end casts"
        // are instant casts and we can use their internal booleans to determine whether it was a success or failure.
        let mut serialized_casts: Vec<SerializedWoWSpellCast> = vec![];

        raw_start_casts.into_iter().for_each(|x| {
            if let Some(spell_id_hashmap) = raw_end_cast_hashmap.get_mut(&x.source_guid) {
                if let Some(inner_vec) = spell_id_hashmap.get_mut(&x.spell_id) {
                    let mut filtered_vec = inner_vec.iter_mut().filter(|x| { !x.used }).collect::<Vec<_>>();
                    let idx = match filtered_vec.binary_search_by(|y| { y.data.log_line.cmp(&x.log_line) }) {
                        Ok(x) => x,
                        Err(x) => x
                    };

                    if idx < filtered_vec.len() {
                        let mut removed = &mut filtered_vec[idx];

                        if let Some(start_id_hashmap) = raw_start_cast_hashmap.get(&x.source_guid) {
                            if let Some(start_inner_vec) = start_id_hashmap.get(&x.spell_id) {
                                // +1 because we're guaranteed to find an exact match so the NEXT cast is at the next index.
                                let start_idx = match start_inner_vec.binary_search_by(|y| { y.cmp(&x.log_line )}) {
                                    Ok(x) => x,
                                    Err(x) => x,
                                } + 1;

                                let valid_pairing = if start_idx < start_inner_vec.len() {
                                    start_inner_vec[start_idx] > removed.data.log_line
                                } else {
                                    true
                                };

                                if valid_pairing {
                                    serialized_casts.push(SerializedWoWSpellCast{
                                        source_guid: x.source_guid,
                                        source_name: x.source_name,
                                        source_flags: x.source_flags,
                                        target_guid: removed.data.target_guid.clone(),
                                        target_name: removed.data.target_name.clone(),
                                        target_flags: removed.data.target_flags,
                                        cast_start: Some(x.tm),
                                        cast_finish: removed.data.tm,
                                        spell_id: removed.data.spell_id,
                                        spell_school: removed.data.spell_school,
                                        success: removed.data.success,
                                        instant: false,
                                    });
                                    removed.used = true;
                                } else {
                                    serialized_casts.push(SerializedWoWSpellCast{
                                        source_guid: x.source_guid,
                                        source_name: x.source_name,
                                        source_flags: x.source_flags,
                                        target_guid: None,
                                        target_name: None,
                                        target_flags: None,
                                        cast_start: Some(x.tm),
                                        cast_finish: x.tm,
                                        spell_id: x.spell_id,
                                        spell_school: x.spell_school,
                                        success: false,
                                        instant: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });

        raw_end_cast_hashmap.into_iter().for_each(|(_source, spell_id_hashmap)| {
            spell_id_hashmap.into_iter().for_each(|(_spell, inner)| {
                inner.into_iter().for_each(|wrapper| {
                    if wrapper.used {
                        return;
                    }
    
                    serialized_casts.push(SerializedWoWSpellCast{
                        source_guid: wrapper.data.source_guid,
                        source_name: wrapper.data.source_name,
                        source_flags: wrapper.data.source_flags,
                        target_guid: wrapper.data.target_guid,
                        target_name: wrapper.data.target_name,
                        target_flags: wrapper.data.target_flags,
                        cast_start: None,
                        cast_finish: wrapper.data.tm,
                        spell_id: wrapper.data.spell_id,
                        spell_school: wrapper.data.spell_school,
                        success: wrapper.data.success,
                        instant: true,
                    });
                });
            });
        });

        // Note that to be consistent with the rest of the event grabber functions we sort by time in ascending order.
        serialized_casts.sort_by(|a, b| {
            a.cast_finish.cmp(&b.cast_finish)
        });

        Ok(serialized_casts)
    }
}

pub async fn list_wow_events_for_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>) -> Result<HttpResponse, SquadOvError> {
    #[derive(Serialize)]
    #[serde(rename_all="camelCase")]
    struct Response {
        deaths: Vec<SerializedWoWDeath>,
        auras: Vec<SerializedWoWAura>,
        encounters: Vec<SerializedWowEncounter>,
        resurrections: Vec<SerializedWoWResurrection>,
        aura_breaks: Vec<SerializedWoWAuraBreak>,
        spell_casts: Vec<SerializedWoWSpellCast>,
    }

    let view_uuid = app.get_wow_match_view_for_user_match(path.user_id, &path.match_uuid).await?.ok_or(SquadOvError::NotFound)?;
    Ok(HttpResponse::Ok().json(Response{
        deaths: app.get_wow_match_death_events(&view_uuid).await?,
        auras: app.get_wow_match_aura_events(&view_uuid).await?,
        encounters: app.get_wow_match_subencounters(&view_uuid).await?,
        resurrections: app.get_wow_match_resurrection_events(&view_uuid).await?,
        aura_breaks: app.get_wow_match_aura_break_events(&view_uuid).await?,
        spell_casts: app.get_wow_match_spell_cast_events(&view_uuid).await?,
    }))
}

#[derive(Deserialize)]
pub struct WowEventIdPath {
    event_id: i64
}

fn default_death_recap_query_seconds() -> i32 {
    return 5
}

#[derive(Deserialize)]
pub struct WowDeathRecapQuery {
    #[serde(default="default_death_recap_query_seconds")]
    seconds: i32
}

pub async fn get_death_recap_handler(app : web::Data<Arc<api::ApiApplication>>, match_path: web::Path<super::WoWUserMatchPath>, event_path: web::Path<WowEventIdPath>, query: web::Query<WowDeathRecapQuery>) -> Result<HttpResponse, SquadOvError> {
    let view_uuid = app.get_wow_match_view_for_user_match(match_path.user_id, &match_path.match_uuid).await?.ok_or(SquadOvError::NotFound)?;
    Ok(HttpResponse::Ok().json(app.get_wow_death_recap(&view_uuid, event_path.event_id, query.seconds).await?))
}