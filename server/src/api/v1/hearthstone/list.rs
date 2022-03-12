use squadov_common::SquadOvError;
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use std::sync::Arc;
use uuid::Uuid;
use squadov_common::hearthstone::{GameType, get_all_hearthstone_game_types};
use serde::{Deserialize};

pub struct HearthstoneMatchListEntry {
    match_uuid: Uuid
}

#[derive(Deserialize)]
pub struct FilteredMatchParameters {
    pub start: i64,
    pub end: i64,
}

impl api::ApiApplication {
    pub async fn list_hearthstone_matches_for_user(&self, user_id: i64, req_user_id: i64, start: i64, end: i64, filters: &[GameType], aux_filters: &super::HearthstoneListQuery) -> Result<Vec<Uuid>, squadov_common::SquadOvError> {
        // Need to inner join on hearthstone_match_metadata as we won't be able to
        // successfully display the match otherwise.
        Ok(sqlx::query_as!(
            HearthstoneMatchListEntry,
            "
            SELECT inp.match_uuid
            FROM (
                SELECT DISTINCT hm.id, hm.match_uuid
                FROM squadov.hearthstone_matches AS hm
                INNER JOIN squadov.hearthstone_match_view AS hmv
                    ON hmv.match_uuid = hm.match_uuid
                INNER JOIN squadov.hearthstone_match_players AS hmp
                    ON hmp.view_uuid = hmv.view_uuid
                INNER JOIN squadov.hearthstone_match_metadata AS hmm
                    ON hmm.view_uuid = hmv.view_uuid
                INNER JOIN squadov.users AS u
                    ON u.id = hmp.user_id
                LEFT JOIN squadov.vods AS v
                    ON v.match_uuid = hm.match_uuid
                        AND v.user_uuid = u.uuid
                        AND v.is_clip = FALSE
                LEFT JOIN squadov.view_share_connections_access_users AS sau
                    ON sau.match_uuid = hm.match_uuid
                        AND sau.user_id = $6
                WHERE hmp.user_id = $1
                    AND hmm.game_type = any($4)
                    AND (NOT $5::BOOLEAN OR v.video_uuid IS NOT NULL)
                    AND ($1 = $6 OR sau.match_uuid IS NOT NULL)
                ORDER BY hm.id DESC, hm.match_uuid
                LIMIT $2 OFFSET $3
            ) as inp
            ",
            user_id,
            end - start,
            start,
            &filters.iter().map(|e| { e.clone() as i32 }).collect::<Vec<i32>>(),
            aux_filters.has_vod.unwrap_or(false),
            req_user_id,
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|e| { e.match_uuid })
            .collect()
        )
    }
}

pub async fn list_hearthstone_matches_for_user_handler(data : web::Path<super::HearthstoneUserMatchInput>, query: web::Query<FilteredMatchParameters>, filters: web::Json<super::HearthstoneListQuery>, app : web::Data<Arc<api::ApiApplication>>, req : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let query = query.into_inner();

    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let matches = app.list_hearthstone_matches_for_user(
        data.user_id,
        session.user.id,
        query.start,
        query.end,
        if filters.game_types.is_empty() {
            get_all_hearthstone_game_types()
        } else {
            &filters.game_types
        },
        &filters,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = matches.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(matches, &req, &api::PaginationParameters{
        start: query.start,
        end: query.end,
    }, expected_total == got_total)?))
}