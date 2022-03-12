use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use serde::Deserialize;
use crate::api;
use crate::api::auth::{SquadOVSession, SquadOVUser};
use std::sync::Arc;
use squadov_common::SquadOvError;
use sqlx::{Transaction, Executor, Postgres};
use chrono::Utc;
use sqlx::Row;

#[derive(Deserialize)]
pub struct CreateSquadInput {
    #[serde(rename="squadName")]
    squad_name: String,
}

impl api::ApiApplication {
    pub async fn create_default_squad(&self, tx: &mut Transaction<'_, Postgres>, user: &SquadOVUser) -> Result<(), SquadOvError> {
        let name = format!("{}'s Squad", &user.username);
        self.create_squad(&mut *tx, &name, user.id, true).await?;

        // Mike
        //self.force_add_user_to_squad(&mut *tx, squad_id, 1).await?;

        // Derek
        //self.force_add_user_to_squad(&mut *tx, squad_id, 4).await?;
        Ok(())
    }

    async fn create_squad(&self, tx: &mut Transaction<'_, Postgres>, squad_name: &str, owner_id: i64, default: bool) -> Result<i64, SquadOvError> {
        let squad_id: i64 = tx.fetch_one(
            sqlx::query!(
                "
                INSERT INTO squadov.squads (
                    squad_name,
                    creation_time,
                    is_default
                )
                VALUES (
                    $1,
                    $2,
                    $3
                )
                RETURNING id
                ",
                squad_name,
                Utc::now(),
                default
            )
        ).await?.get(0);

        tx.execute(
            sqlx::query!(
                "
                INSERT INTO squadov.squad_role_assignments (
                    squad_id,
                    user_id,
                    squad_role
                )
                VALUES (
                    $1,
                    $2,
                    'Owner'
                )
                ",
                squad_id,
                owner_id
            )
        ).await?;

        Ok(squad_id)
    }
}

pub async fn create_squad_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<CreateSquadInput>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let squad_id = app.create_squad(&mut tx, &data.squad_name, session.user.id, false).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(squad_id))
}