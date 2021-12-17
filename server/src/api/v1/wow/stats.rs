use actix_web::{web, HttpResponse};
use crate::api;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime,Utc};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct WowStatsQueryParams {
    // How often we sample the start - end time range.
    #[serde(rename="psStepSeconds")]
    ps_step_seconds: i64,
    #[serde(deserialize_with="squadov_common::parse_utc_time_from_milliseconds")]
    start: Option<DateTime<Utc>>,
    #[serde(deserialize_with="squadov_common::parse_utc_time_from_milliseconds")]
    end: Option<DateTime<Utc>>
}

#[derive(Serialize)]
pub struct WowStatDatum {
    tm: f64,
    value: f64
}

#[derive(Serialize)]
pub struct WowStatItem {
    guid: String,
    value: i64,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct WowMatchStatSummaryData {
    pub damage_dealt: Vec<WowStatItem>,
    pub damage_received: Vec<WowStatItem>,
    pub heals: Vec<WowStatItem>,
}

impl api::ApiApplication {
    async fn get_wow_match_dps(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String], params: &WowStatsQueryParams) -> Result<HashMap<String, Vec<WowStatDatum>>, SquadOvError> {
        if params.start.is_none() || params.end.is_none() {
            return Err(SquadOvError::BadRequest);
        }

        let mut ret_map: HashMap<String, Vec<WowStatDatum>> = HashMap::new();
        sqlx::query!(
            r#"
            SELECT
                FLOOR((EXTRACT(EPOCH FROM wve.tm) - EXTRACT(EPOCH FROM $4::TIMESTAMPTZ)) / $6::BIGINT) * $6::BIGINT AS "xtm",
                COALESCE(wcp.owner_guid, wcp.unit_guid) AS "guid",
                SUM(wde.amount) / CAST($6::BIGINT AS DOUBLE PRECISION) AS "amount"
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.view_id = wmv.alt_id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.character_id = wve.source_char
            INNER JOIN squadov.wow_match_view_damage_events AS wde
                ON wde.event_id = wve.event_id
            WHERE wmv.match_uuid = $1
                AND wmv.user_id = $2
                AND wcp.unit_guid = ANY($3)
                AND wve.tm >= $4 AND wve.tm <= $5
            GROUP BY xtm, guid
            ORDER BY xtm, guid
            "#,
            match_uuid,
            user_id,
            combatant_guids,
            params.start.unwrap(),
            params.end.unwrap(),
            params.ps_step_seconds,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .for_each(|x| {
                let guid = x.guid.unwrap();
                let amount = x.amount.unwrap();
                let tm = x.xtm.unwrap();

                if !ret_map.contains_key(&guid) {
                    ret_map.insert(guid.clone(), vec![]);
                }
    
                let inner_vec = ret_map.get_mut(&guid).unwrap();
                inner_vec.push(WowStatDatum{
                    tm: tm,
                    value: amount,
                });
            });

        Ok(ret_map)
    }

    async fn get_wow_match_heals_per_second(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String], params: &WowStatsQueryParams) -> Result<HashMap<String, Vec<WowStatDatum>>, SquadOvError> {
        if params.start.is_none() || params.end.is_none() {
            return Err(SquadOvError::BadRequest);
        }

        let mut ret_map: HashMap<String, Vec<WowStatDatum>> = HashMap::new();
        sqlx::query!(
            r#"
            SELECT
                FLOOR((EXTRACT(EPOCH FROM wve.tm) - EXTRACT(EPOCH FROM $4::TIMESTAMPTZ)) / $6::BIGINT) * $6::BIGINT AS "xtm",
                COALESCE(wcp.owner_guid, wcp.unit_guid) AS "guid",
                SUM(GREATEST(whe.amount - whe.overheal, 0)) / CAST($6::BIGINT AS DOUBLE PRECISION) AS "amount"
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.view_id = wmv.alt_id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.character_id = wve.source_char
            INNER JOIN squadov.wow_match_view_healing_events AS whe
                ON whe.event_id = wve.event_id
            WHERE wmv.match_uuid = $1
                AND wmv.user_id = $2
                AND wcp.unit_guid = ANY($3)
                AND wve.tm >= $4 AND wve.tm <= $5
            GROUP BY xtm, guid
            ORDER BY xtm, guid
            "#,
            match_uuid,
            user_id,
            combatant_guids,
            params.start.unwrap(),
            params.end.unwrap(),
            params.ps_step_seconds,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .for_each(|x| {
                let guid = x.guid.unwrap();
                let amount = x.amount.unwrap();
                let tm = x.xtm.unwrap();

                if !ret_map.contains_key(&guid) {
                    ret_map.insert(guid.clone(), vec![]);
                }
    
                let inner_vec = ret_map.get_mut(&guid).unwrap();
                inner_vec.push(WowStatDatum{
                    tm: tm,
                    value: amount,
                });
            });

        Ok(ret_map)
    }

    async fn get_wow_match_damage_received_per_second(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String], params: &WowStatsQueryParams) -> Result<HashMap<String, Vec<WowStatDatum>>, SquadOvError> {
        if params.start.is_none() || params.end.is_none() {
            return Err(SquadOvError::BadRequest);
        }

        let mut ret_map: HashMap<String, Vec<WowStatDatum>> = HashMap::new();
        sqlx::query!(
            r#"
            SELECT
                FLOOR((EXTRACT(EPOCH FROM wve.tm) - EXTRACT(EPOCH FROM $4::TIMESTAMPTZ)) / $6::BIGINT) * $6::BIGINT AS "xtm",
                COALESCE(wcp.owner_guid, wcp.unit_guid) AS "guid",
                SUM(wde.amount) / CAST($6::BIGINT AS DOUBLE PRECISION) AS "amount"
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.view_id = wmv.alt_id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.character_id = wve.dest_char
            INNER JOIN squadov.wow_match_view_damage_events AS wde
                ON wde.event_id = wve.event_id
            WHERE wmv.match_uuid = $1
                AND wmv.user_id = $2
                AND wcp.unit_guid = ANY($3)
                AND wve.tm >= $4 AND wve.tm <= $5
            GROUP BY xtm, guid
            ORDER BY xtm, guid
            "#,
            match_uuid,
            user_id,
            combatant_guids,
            params.start.unwrap(),
            params.end.unwrap(),
            params.ps_step_seconds,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .for_each(|x| {
                let guid = x.guid.unwrap();
                let amount = x.amount.unwrap();
                let tm = x.xtm.unwrap();

                if !ret_map.contains_key(&guid) {
                    ret_map.insert(guid.clone(), vec![]);
                }
    
                let inner_vec = ret_map.get_mut(&guid).unwrap();
                inner_vec.push(WowStatDatum{
                    tm: tm,
                    value: amount,
                });
            });

        Ok(ret_map)
    }

    async fn get_wow_summary_damage_dealt(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String])  -> Result<Vec<WowStatItem>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                WowStatItem,
                r#"
                SELECT
                    wcp.unit_guid AS "guid!",
                    SUM(wde.amount) AS "value!"
                FROM squadov.wow_match_view AS wmv
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.view_id = wmv.alt_id
                INNER JOIN squadov.wow_match_view_character_presence AS wcp
                    ON wcp.character_id = wve.source_char
                INNER JOIN squadov.wow_match_view_damage_events AS wde
                    ON wde.event_id = wve.event_id
                WHERE wmv.match_uuid = $1
                    AND wmv.user_id = $2
                    AND wcp.unit_guid = ANY($3)
                GROUP BY wcp.unit_guid
                "#,
                match_uuid,
                user_id,
                combatant_guids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn get_wow_summary_damage_received(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String])  -> Result<Vec<WowStatItem>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                WowStatItem,
                r#"
                SELECT
                    wcp.unit_guid AS "guid!",
                    SUM(wde.amount) AS "value!"
                FROM squadov.wow_match_view AS wmv
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.view_id = wmv.alt_id
                INNER JOIN squadov.wow_match_view_character_presence AS wcp
                    ON wcp.character_id = wve.dest_char
                INNER JOIN squadov.wow_match_view_damage_events AS wde
                    ON wde.event_id = wve.event_id
                WHERE wmv.match_uuid = $1
                    AND wmv.user_id = $2
                    AND wcp.unit_guid = ANY($3)
                GROUP BY wcp.unit_guid
                "#,
                match_uuid,
                user_id,
                combatant_guids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn get_wow_summary_heals(&self, user_id: i64, match_uuid: &Uuid, combatant_guids: &[String])  -> Result<Vec<WowStatItem>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                WowStatItem,
                r#"
                SELECT
                    wcp.unit_guid AS "guid!",
                    SUM(GREATEST(whe.amount - whe.overheal, 0)) AS "value!"
                FROM squadov.wow_match_view AS wmv
                INNER JOIN squadov.wow_match_view_events AS wve
                    ON wve.view_id = wmv.alt_id
                INNER JOIN squadov.wow_match_view_character_presence AS wcp
                    ON wcp.character_id = wve.source_char
                INNER JOIN squadov.wow_match_view_healing_events AS whe
                    ON whe.event_id = wve.event_id
                WHERE wmv.match_uuid = $1
                    AND wmv.user_id = $2
                    AND wcp.unit_guid = ANY($3)
                GROUP BY wcp.unit_guid
                "#,
                match_uuid,
                user_id,
                combatant_guids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }
}

pub async fn get_wow_match_dps_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>, query: web::Query<WowStatsQueryParams>) -> Result<HttpResponse, SquadOvError> {
    let chars: Vec<_> = app.list_wow_characters_for_match(&path.match_uuid, path.user_id).await?.into_iter().map(|x| { x.guid }).collect();
    let stats = app.get_wow_match_dps(path.user_id, &path.match_uuid, &chars, &query).await?;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn get_wow_match_heals_per_second_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>, query: web::Query<WowStatsQueryParams>) -> Result<HttpResponse, SquadOvError> {
    let chars: Vec<_> = app.list_wow_characters_for_match(&path.match_uuid, path.user_id).await?.into_iter().map(|x| { x.guid }).collect();
    let stats = app.get_wow_match_heals_per_second(path.user_id, &path.match_uuid, &chars, &query).await?;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn get_wow_match_damage_received_per_second_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>, query: web::Query<WowStatsQueryParams>) -> Result<HttpResponse, SquadOvError> {
    let chars: Vec<_> = app.list_wow_characters_for_match(&path.match_uuid, path.user_id).await?.into_iter().map(|x| { x.guid }).collect();
    let stats = app.get_wow_match_damage_received_per_second(path.user_id, &path.match_uuid, &chars, &query).await?;
    Ok(HttpResponse::Ok().json(stats))
}

pub async fn get_wow_match_stat_summary_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>) -> Result<HttpResponse, SquadOvError> {
    let chars: Vec<_> = app.list_wow_characters_for_match(&path.match_uuid, path.user_id).await?.into_iter().map(|x| { x.guid }).collect();
    Ok(
        HttpResponse::Ok().json(&WowMatchStatSummaryData{
            damage_dealt: app.get_wow_summary_damage_dealt(path.user_id, &path.match_uuid, &chars).await?,
            damage_received: app.get_wow_summary_damage_received(path.user_id, &path.match_uuid, &chars).await?,
            heals: app.get_wow_summary_heals(path.user_id, &path.match_uuid, &chars).await?,
        })
    )
}