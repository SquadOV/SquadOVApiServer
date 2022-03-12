use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use std::sync::Arc;
use squadov_common::{SquadOvError, SquadRole};
use crate::api::auth::SquadOVSession;
use sqlx::{Transaction, Postgres};
use uuid::Uuid;

impl api::ApiApplication {
    async fn delete_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.squads
            WHERE id = $1
            ",
            squad_id
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    async fn leave_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.squad_role_assignments
            WHERE squad_id = $1 AND user_id = $2
            ",
            squad_id,
            user_id
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    async fn remove_content_from_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, video_uuid: &Uuid) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.share_match_vod_connections
            WHERE video_uuid = $1 AND dest_squad_id = $2
            ",
            video_uuid,
            squad_id,
        )
            .execute(tx)
            .await?;
        Ok(())
    }
}

pub async fn delete_squad_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.delete_squad(&mut tx, data.squad_id).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

async fn generic_remove_user_from_squad_handler(app : web::Data<Arc<api::ApiApplication>>, user_id: i64, squad_id: i64) -> Result<(), SquadOvError> {
    // Ensure that the owner user isn't trying to leave/be kicked. Is there a way to
    // get this to be represented as a PostgreSQL constraint so that DB
    // operation just fails instead?
    let role = app.get_squad_user_role(squad_id, user_id).await?;
    if role.is_some() && role.unwrap() == SquadRole::Owner {
        return Err(SquadOvError::BadRequest);
    }

    let mut tx = app.pool.begin().await?;
    app.leave_squad(&mut tx, squad_id, user_id).await?;
    tx.commit().await?;

    Ok(())
}

pub async fn leave_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    generic_remove_user_from_squad_handler(app, session.user.id, path.squad_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn kick_squad_member_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadMembershipPathInput>) -> Result<HttpResponse, SquadOvError> {   
    generic_remove_user_from_squad_handler(app, path.user_id, path.squad_id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn remove_content_from_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadContentInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.remove_content_from_squad(&mut tx, path.squad_id, &path.video_uuid).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}