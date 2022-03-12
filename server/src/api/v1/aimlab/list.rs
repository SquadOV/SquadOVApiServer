use squadov_common::{
    SquadOvError,
    AimlabTask,
};
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AimlabUserTaskListInput {
    user_id: i64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct AimlabListQuery {
    tasks: Option<Vec<String>>,
    has_vod: Option<bool>,
}

impl api::ApiApplication {
    pub async fn list_aimlab_matches_for_user(&self, user_id: i64, req_user_id: i64, start: i64, end: i64, filter: &AimlabListQuery) -> Result<Vec<AimlabTask>, SquadOvError> {
        let matches = sqlx::query_as!(
            AimlabTask,
            "
            SELECT DISTINCT ON (at.create_date, at.match_uuid) at.*
            FROM squadov.aimlab_tasks AS at
            INNER JOIN squadov.users AS u
                ON u.id = at.user_id
            LEFT JOIN squadov.vods AS v
                ON v.match_uuid = at.match_uuid
                    AND v.user_uuid = u.uuid
                    AND v.is_clip = FALSE
            LEFT JOIN squadov.view_share_connections_access_users AS sau
                ON sau.match_uuid = at.match_uuid
                    AND sau.user_id = $6
            WHERE at.user_id = $1
                AND (CARDINALITY($4::VARCHAR[]) = 0 OR at.task_name = ANY($4))
                AND (NOT $5::BOOLEAN OR v.video_uuid IS NOT NULL)
                AND ($1 = $6 OR sau.match_uuid IS NOT NULL)
            ORDER BY at.create_date DESC, at.match_uuid
            LIMIT $2 OFFSET $3
            ",
            user_id,
            end - start,
            start,
            &filter.tasks.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            filter.has_vod.unwrap_or(false),
            req_user_id,
        )
            .fetch_all(&*self.pool)
            .await?;
        return Ok(matches);
    }

    pub async fn list_aimlab_matches_for_uuids(&self, uuids: &[Uuid]) -> Result<Vec<AimlabTask>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                AimlabTask,
                "
                SELECT *
                FROM squadov.aimlab_tasks
                WHERE match_uuid = ANY($1)
                ",
                uuids
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }
}

pub async fn list_aimlab_matches_for_user_handler(data : web::Path<AimlabUserTaskListInput>, query: web::Query<api::PaginationParameters>, filter: web::Json<AimlabListQuery>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, squadov_common::SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let query = query.into_inner();
    let matches = app.list_aimlab_matches_for_user(
        data.user_id,
        session.user.id,
        query.start,
        query.end,
        &filter,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = matches.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(matches, &req, &query, expected_total == got_total)?)) 
}