use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::{
    auth::SquadOVSession,
};
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    discord::{
        self,
    }
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DiscordIdPath {
    pub discord_snowflake: i64,
}

pub async fn delete_linked_discord_account_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<DiscordIdPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    discord::db::unlink_discord_account_for_user(&*app.pool, session.user.id, path.discord_snowflake).await?;
    Ok(HttpResponse::NoContent().finish())
}
