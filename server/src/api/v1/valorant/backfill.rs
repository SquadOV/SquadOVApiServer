use squadov_common::{
    SquadOvError,
    riot::{
        db,
    },
};
use crate::api;
use actix_web::{web, HttpResponse};
use std::sync::Arc;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ValorantBackfillPath {
    user_id: i64
}

#[derive(Deserialize)]
pub struct ValorantBackfillData {
    puuid: String,
}

pub async fn request_valorant_match_backfill_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<ValorantBackfillPath>, data: web::Json<ValorantBackfillData>) -> Result<HttpResponse, SquadOvError> {
    // Ensure that the user is linked to this particular account before firing off a backfill request.
    let account = db::get_user_riot_account_from_raw_puuid(&*app.pool, path.user_id, &data.puuid).await?.ok_or(SquadOvError::NotFound)?;
    app.valorant_itf.request_backfill_user_valorant_matches(&account.puuid).await?;
    Ok(HttpResponse::Ok().finish())
}