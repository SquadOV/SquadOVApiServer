use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use squadov_common::{
    SquadOvError,
};
use std::sync::Arc;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SquadmateQuery {
    pub squads: Option<Vec<i64>>,
}

pub async fn get_user_squadmates_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Json<SquadmateQuery>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(squadov_common::SquadOvError::BadRequest),
    };

    let user_ids = app.get_user_ids_in_same_squad_as_users(&vec![session.user.id], query.squads.as_ref()).await?;
    let handles = app.get_user_handles(&user_ids).await?;
    Ok(HttpResponse::Ok().json(handles))
}