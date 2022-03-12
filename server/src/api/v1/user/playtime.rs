use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use squadov_common::{
    SquadOvError,
};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Deserialize)]
pub struct PlaytimeRequestQuery {
    seconds: i64
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeInfo {
    aimlab_ms: i64,
    csgo_ms: i64,
    hearthstone_ms: i64,
    lol_ms: i64,
    tft_ms: i64,
    valorant_ms: i64,
    wow_ms: i64,
}

impl api::ApiApplication {
    async fn get_user_recorded_playtime(&self, user_id: i64, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Result<PlaytimeInfo, SquadOvError> {
        Ok(
            sqlx::query_as!(
                PlaytimeInfo,
                r#"
                WITH relevant_matches(match_uuid, duration) AS (
                    SELECT
                        v.match_uuid,
                        (EXTRACT(EPOCH FROM v.end_time) - EXTRACT(EPOCH FROM v.start_time)) * 1000.0
                    FROM squadov.vods AS v
                    INNER JOIN squadov.users AS u
                        ON u.uuid = v.user_uuid
                    WHERE u.id = $1
                        AND start_time BETWEEN $2 AND $3
                        AND match_uuid IS NOT NULL
                        AND user_uuid IS NOT NULL
                        AND start_time IS NOT NULL
                        AND end_time IS NOT NULL
                )
                SELECT 
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.aimlab_tasks AS at
                            ON at.match_uuid = rm.match_uuid
                    ) AS "aimlab_ms!",
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.csgo_matches AS cm
                            ON cm.match_uuid = rm.match_uuid
                    ) AS "csgo_ms!",
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.hearthstone_matches AS hm
                            ON hm.match_uuid = rm.match_uuid
                    ) AS "hearthstone_ms!",
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.lol_matches AS lm
                            ON lm.match_uuid = rm.match_uuid
                    ) AS "lol_ms!",
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.tft_matches AS tm
                            ON tm.match_uuid = rm.match_uuid
                    ) AS "tft_ms!",
                    (
                        SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                        FROM relevant_matches AS rm
                        INNER JOIN squadov.valorant_matches AS vm
                            ON vm.match_uuid = rm.match_uuid
                    ) AS "valorant_ms!",
                    (
                        SELECT (
                            SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                            FROM relevant_matches AS rm
                            INNER JOIN squadov.new_wow_challenges AS wc
                                ON wc.match_uuid = rm.match_uuid
                        ) + (
                            SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                            FROM relevant_matches AS rm
                            INNER JOIN squadov.new_wow_encounters AS we
                                ON we.match_uuid = rm.match_uuid
                        ) + (
                            SELECT COALESCE(SUM(rm.duration)::BIGINT,0)
                            FROM relevant_matches AS rm
                            INNER JOIN squadov.new_wow_arenas AS wa
                                ON wa.match_uuid = rm.match_uuid
                        )
                    ) AS "wow_ms!"
                "#,
                user_id,
                start_time,
                end_time,
            )
                .fetch_one(&*self.pool)
                .await?
        )
    }
}

pub async fn get_user_recorded_playtime_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<PlaytimeRequestQuery>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let now = Utc::now();
    let playtime = app.get_user_recorded_playtime(
        session.user.id,
        now - chrono::Duration::seconds(query.seconds),
        now,
    ).await?;

    Ok(HttpResponse::Ok().json(&playtime))
}