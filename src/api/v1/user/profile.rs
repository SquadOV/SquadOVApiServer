use crate::common;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use crate::api;

#[derive(Deserialize)]
pub struct ProfileResource {
    user_id: i64,
}

pub async fn get_user_profile_handler(data : web::Path<ProfileResource>, app : web::Data<api::ApiApplication>) -> Result<HttpResponse, common::SquadOvError> {
    match app.users.get_stored_user_from_id(data.user_id, &app.pool).await {
        Ok(x) => match x {
            Some(x) => Ok(HttpResponse::Ok().json(&x)),
            None => Err(common::SquadOvError::NotFound),
        },
        Err(err) => Err(common::SquadOvError::InternalError(format!("Get User Profile Handler {}", err))),
    }
}