use squadov_common::{
    SquadOvError,
    riot::{
        db,
        ValorantMatchFilters,
    },
};
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ValorantUserMatchListInput {
    user_id: i64,
    puuid: String,
}

pub async fn list_valorant_matches_for_user_handler(data : web::Path<ValorantUserMatchListInput>, query: web::Query<api::PaginationParameters>, filters: web::Json<ValorantMatchFilters>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let user = app.users.get_stored_user_from_id(data.user_id, &*app.pool).await?.ok_or(SquadOvError::NotFound)?;
    let query = query.into_inner();
    let matches = db::list_valorant_match_summaries_for_puuid(
        &*app.pool,
        &data.puuid,
        &user.uuid,
        session.user.id,
        query.start,
        query.end,
        &filters,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = matches.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(matches, &req, &query, expected_total == got_total)?)) 
}