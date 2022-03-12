use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use std::sync::Arc;
use crate::{
    api,
    api::auth::SquadOVSession,
};
use squadov_common::{
    SquadOvError,
    accounts::{
        twitch
    }
};

pub async fn get_my_linked_twitch_account_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(
        twitch::find_twitch_accounts_for_user(&*app.pool, session.user.id).await?
    ))
}