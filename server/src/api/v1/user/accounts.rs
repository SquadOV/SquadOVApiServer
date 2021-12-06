mod riot;
mod twitch;

pub use riot::*;
pub use twitch::*;

use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use crate::api::auth::{SquadOVUser, SquadOVUserHandle, SquadOVSession};
use squadov_common::{
    SquadOvError,
    accounts::{
        twitch as twitch_acc,
        TwitchAccount,
    },
    riot::{
        RiotAccount,
        db as riotdb,
    },
    discord::{
        DiscordUser,
        db as discorddb,
    },
};
use uuid::Uuid;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;

impl api::ApiApplication {
    pub async fn get_user_handles(&self, ids: &[i64]) -> Result<Vec<SquadOVUserHandle>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                SquadOVUserHandle,
                "
                SELECT u.id, u.username
                FROM squadov.users AS u
                WHERE u.id = ANY($1)
                ",
                ids,
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

    pub async fn user_id_to_uuid(&self, user_id: i64) -> Result<Uuid, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT uuid
                FROM squadov.users AS u
                WHERE u.id = $1
                ",
                user_id,
            )
                .fetch_one(&*self.pool)
                .await?
                .uuid
        )
    }

    pub async fn get_user_uuid_to_user_id_map(&self, uuids: &[Uuid]) -> Result<HashMap<Uuid, i64>, SquadOvError> {
        Ok(sqlx::query!(
            "
            SELECT u.uuid, u.id
            FROM squadov.users AS u
            WHERE u.uuid = any($1)
            ",
            uuids
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| { (x.uuid, x.id) } )
            .collect())
    }


    pub async fn update_user_registration_time(&self, user_id: i64, tm: &DateTime<Utc>) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.users
            SET registration_time = $2
            WHERE id = $1
            ",
            user_id,
            tm,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn send_welcome_email_to_user(&self, user: &SquadOVUser) -> Result<(), SquadOvError> {
        // Keeping this for legacy. But note that the welcome email is now sent based on
        // user analytics (Segment + Vero) based off the registered event.
        sqlx::query!(
            "
            UPDATE squadov.users
            SET welcome_sent = TRUE
            WHERE id = $1
            ",
            user.id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}

#[derive(Serialize)]
struct AllAccountsResponse {
    riot: Vec<RiotAccount>,
    twitch: Vec<TwitchAccount>,
    discord: Vec<DiscordUser>,
}

pub async fn get_all_my_linked_accounts_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(AllAccountsResponse{
        riot: riotdb::list_riot_accounts_for_user(&*app.pool, session.user.id).await?,
        twitch: twitch_acc::find_twitch_accounts_for_user(&*app.pool, session.user.id).await?,
        discord: discorddb::find_discord_accounts_for_user(&*app.pool, session.user.id).await?,
    }))
}