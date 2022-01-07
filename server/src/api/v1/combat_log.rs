use actix_web::{web, HttpResponse};
use squadov_common::SquadOvError;
use crate::api::ApiApplication;
use std::sync::Arc;

pub async fn get_combatlog_config_handler(app : web::Data<Arc<ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().finish())
}