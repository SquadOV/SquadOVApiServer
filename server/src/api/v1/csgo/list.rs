use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use squadov_common::{
    SquadOvError,
    csgo::{
        CsgoListQuery,
        db,
    },
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct CsgoUserMatchListInput {
    user_id: i64,
}

pub async fn list_csgo_matches_for_user_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<CsgoUserMatchListInput>, query: web::Query<api::PaginationParameters>, filter: web::Json<CsgoListQuery>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let query = query.into_inner();
    let matches = db::list_csgo_match_summaries_for_user(
        &*app.pool,
        path.user_id,
        session.user.id,
        query.start,
        query.end,
        &filter,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = matches.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(matches, &req, &query, expected_total == got_total)?)) 
}