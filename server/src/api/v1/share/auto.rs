use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use squadov_common::{
    SquadOvError,
    share::{
        auto,
        auto::AutoShareConnection,
    }
};
use std::sync::Arc;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AutoConnectionPathInput {
    setting_id: i64,
}

pub async fn get_auto_share_settings_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(
        auto::get_auto_share_connections_for_user(&*app.pool, session.user.id).await?
    ))
}

pub async fn new_auto_share_setting_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<AutoShareConnection>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let mut tx = app.pool.begin().await?;
    let conn = auto::create_auto_share_connection_for_user(&mut tx, data.into_inner(), session.user.id).await?;
    auto::link_auto_share_connection_to_games_for_user(&mut tx, session.user.id, conn.id, &conn.games).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(conn))
}

pub async fn delete_auto_share_setting_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<AutoConnectionPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    auto::delete_auto_share_connection_for_user(&*app.pool, session.user.id, path.setting_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn edit_auto_share_setting_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<AutoConnectionPathInput>, data: web::Json<AutoShareConnection>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let edit_data = AutoShareConnection{
        id: path.setting_id,
        source_user_id: session.user.id,
        ..data.into_inner()
    };

    let mut tx = app.pool.begin().await?;
    auto::edit_auto_share_connection_for_user(&mut tx, &edit_data, session.user.id).await?;
    auto::delete_auto_share_connection_to_games_for_user(&mut tx, session.user.id, path.setting_id).await?;
    auto::link_auto_share_connection_to_games_for_user(&mut tx, session.user.id, edit_data.id, &edit_data.games).await?;
    tx.commit().await?;

    Ok(HttpResponse::NoContent().finish())
}