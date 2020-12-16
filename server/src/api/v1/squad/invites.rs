use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use crate::api::v1::UserResourcePath;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use squadov_common::{SquadOvError, SquadInvite};
use sqlx::{Transaction, Executor, Postgres, Row};
use serde::Deserialize;
use chrono::Utc;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateSquadInviteInput {
    usernames: Vec<String>
}

impl api::ApiApplication {
    pub async fn get_squad_invite_user(&self, invite_uuid: &Uuid) -> Result<i64, SquadOvError> {
        Ok(sqlx::query_scalar(
            "
            SELECT user_id
            FROM squadov.squad_membership_invites
            WHERE invite_uuid = $1
            "
        )
            .bind(invite_uuid)
            .fetch_one(&*self.pool)
            .await?)
    }

    async fn create_squad_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, inviter_user_id: i64, usernames: &[String]) -> Result<(), SquadOvError> {
        if usernames.is_empty() {
            return Err(SquadOvError::BadRequest);
        }

        // For every username, we grab their user ID and every squad they're in.
        // For each squad, we check to see if they're in the squad in question.
        // We do not send invites to users who are alreayd in the squad.
        let user_ids_usernames: HashMap<String, (i64, bool)> = tx.fetch_all(
            sqlx::query(
                "
                SELECT us.id, us.username, sra.squad_id
                FROM squadov.users AS us
                LEFT JOIN squadov.squad_role_assignments AS sra
                    ON sra.user_id = us.id
                WHERE us.username = any($1)
                "
            )
                .bind(usernames)
        ).await?.into_iter().map(|x| {
            (x.get::<String, usize>(1), (x.get::<i64, usize>(0), x.get::<Option<i64>, usize>(2).unwrap_or(-1) == squad_id))
        }).collect();

        let user_ids: Vec<i64> = usernames.iter().filter(|x| {
            user_ids_usernames.contains_key(&x[..]) &&
                !user_ids_usernames.get(&x[..]).unwrap().1
        }).map(|x| {
            user_ids_usernames.get(x).unwrap().0
        }).collect();

        if user_ids.is_empty() {
            return Err(SquadOvError::BadRequest);
        }

        let mut sql: Vec<String> = Vec::new();
        let now = Utc::now();

        sql.push(String::from(
            "
            INSERT INTO squadov.squad_membership_invites(
                squad_id,
                user_id,
                invite_time,
                inviter_user_id
            ) VALUES
            "
        ));

        for uid in user_ids {
            sql.push(format!("
                (
                    {},
                    {},
                    {},
                    {}
                )",
                squad_id,
                uid,
                squadov_common::sql_format_time(&now),
                inviter_user_id,
            ));
            sql.push(String::from(","));
        }
        sql.truncate(sql.len() - 1);
        sqlx::query(&sql.join(" ")).execute(tx).await?;

        // TODO #13: Send squad invite emails once we've successfully tracked them in the database.
        // Any invite that doesn't get sent (e.g. an error occurs during sending) should be ignored as
        // we should just force the user to deal with an unreceived invite (email) and resending the invite
        // if necessary.
        Ok(())
    }

    pub async fn delete_squad_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.squad_membership_invites
            WHERE squad_id = $1 AND invite_uuid = $2 AND response_time IS NULL
            ",
            squad_id,
            invite_uuid
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn accept_reject_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid, accepted: bool) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.squad_membership_invites
            SET joined = $3,
                response_time = $4
            WHERE squad_id = $1 AND invite_uuid = $2 AND response_time IS NULL
            RETURNING invite_uuid
            ",
            squad_id,
            invite_uuid,
            accepted,
            Utc::now(),
        )
            // Do a fetch one here to error if we try to accept/reject an already used invite.
            .fetch_one(tx)
            .await?;
        Ok(())
    }

    pub async fn add_user_to_squad_from_invite(&self, tx: &mut Transaction<'_, Postgres>, squad_id: i64, invite_uuid: &Uuid) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.squad_role_assignments (
                squad_id,
                user_id,
                squad_role
            )
            SELECT $1, user_id, 'Member'
            FROM squadov.squad_membership_invites
            WHERE squad_id = $1 AND invite_uuid = $2
            ",
            squad_id,
            invite_uuid
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn get_user_squad_invites(&self, user_id: i64) -> Result<Vec<SquadInvite>, SquadOvError> {
        Ok(sqlx::query_as!(
            SquadInvite,
            r#"
            SELECT
                smi.squad_id,
                smi.user_id,
                smi.joined,
                smi.response_time,
                smi.invite_time,
                smi.invite_uuid,
                ur.username AS "username",
                us.username AS "inviter_username"
            FROM squadov.squad_membership_invites AS smi
            INNER JOIN squadov.users AS us
                ON us.id = smi.inviter_user_id
            INNER JOIN squadov.users AS ur
                ON ur.id = smi.user_id
            WHERE user_id = $1 AND response_time IS NULL
            "#,
            user_id
        )
            .fetch_all(&*self.pool)
            .await?)
    }

    pub async fn get_squad_invites(&self, squad_id: i64) -> Result<Vec<SquadInvite>, SquadOvError> {
        Ok(sqlx::query_as!(
            SquadInvite,
            r#"
            SELECT
                smi.squad_id,
                smi.user_id,
                smi.joined,
                smi.response_time,
                smi.invite_time,
                smi.invite_uuid,
                ur.username AS "username",
                us.username AS "inviter_username"
            FROM squadov.squad_membership_invites AS smi
            INNER JOIN squadov.users AS us
                ON us.id = smi.inviter_user_id
            INNER JOIN squadov.users AS ur
                ON ur.id = smi.user_id
            WHERE smi.squad_id = $1 AND response_time IS NULL
            "#,
            squad_id
        )
            .fetch_all(&*self.pool)
            .await?)
    }
}

pub async fn create_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadSelectionInput>, data: web::Json<CreateSquadInviteInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    app.create_squad_invite(&mut tx, path.squad_id, session.user.id, &data.usernames).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn accept_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.accept_reject_invite(&mut tx, path.squad_id, &path.invite_uuid, true).await?;
    app.add_user_to_squad_from_invite(&mut tx, path.squad_id, &path.invite_uuid).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn reject_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.accept_reject_invite(&mut tx, path.squad_id, &path.invite_uuid, false).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn get_user_squad_invites_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<UserResourcePath>) -> Result<HttpResponse, SquadOvError> {
    let invites = app.get_user_squad_invites(path.user_id).await?;
    Ok(HttpResponse::Ok().json(&invites))
}

pub async fn  get_all_squad_invites_handler(app : web::Data<Arc<api::ApiApplication>>, path : web::Path<super::SquadSelectionInput>) -> Result<HttpResponse, SquadOvError> {
    let invites = app.get_squad_invites(path.squad_id).await?;
    Ok(HttpResponse::Ok().json(&invites))
}

pub async fn revoke_squad_invite_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::SquadInviteInput>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.delete_squad_invite(&mut tx, path.squad_id, &path.invite_uuid).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}