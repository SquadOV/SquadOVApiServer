use actix_web::{web, HttpResponse};
use crate::api;
use std::sync::Arc;
use squadov_common::SquadOvError;
use sqlx::{Transaction, Postgres};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EditSquadInput {
    #[serde(rename="squadName")]
    squad_name: String
}

impl api::ApiApplication {
    async fn edit_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, squad_name: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.squads
            SET squad_name = $2
            WHERE id = $1
            ",
            squad_id,
            squad_name
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    async fn add_squad_user_share_blacklist(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.squad_user_share_blacklist (
                squad_id,
                user_id
            ) VALUES (
                $1,
                $2
            ) ON CONFLICT DO NOTHING
            ",
            squad_id,
            user_id
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    async fn remove_squad_user_share_blacklist(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.squad_user_share_blacklist
            WHERE squad_id = $1
                AND user_id = $2
            ",
            squad_id,
            user_id
        )
            .execute(tx)
            .await?;
        Ok(())
    }
}

pub async fn edit_squad_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>, data: web::Json<EditSquadInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.edit_squad(&mut tx, path.squad_id, &data.squad_name).await?;
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct CanShareData {
    pub can_share: bool,
}

pub async fn change_squad_member_can_share_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadMembershipPathInput>, data: web::Json<CanShareData>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    if data.can_share {
        app.remove_squad_user_share_blacklist(&mut tx, path.squad_id, path.user_id).await?;
    } else {
        app.add_squad_user_share_blacklist(&mut tx, path.squad_id, path.user_id).await?;
    }
    tx.commit().await?;
    Ok(HttpResponse::NoContent().finish())
}