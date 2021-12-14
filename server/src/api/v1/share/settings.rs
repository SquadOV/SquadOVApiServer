use crate::api;
use crate::api::auth::SquadOVSession;
use serde::{Serialize, Deserialize};
use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    share::MatchVideoShareConnection,
};
use std::sync::Arc;
use sqlx::{Transaction, Executor, Postgres};
use std::convert::TryFrom;

#[derive(Copy, Clone, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all="camelCase")]
pub struct AutoShareSetting {
    share_on_join: bool
}

impl api::ApiApplication {
    pub async fn create_auto_share_settings_for_user_if_not_exist(&self, tx: &mut Transaction<'_, Postgres>, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.user_autoshare_common_settings (user_id)
            VALUES ($1)
            ON CONFLICT DO NOTHING
            ",
            user_id
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn get_user_auto_share_settings<'a, T>(&self, ex: T, user_id: i64) -> Result<AutoShareSetting, SquadOvError>
    where
        T: Executor<'a, Database = Postgres>
    {
        Ok(
            sqlx::query_as!(
                AutoShareSetting,
                "
                SELECT share_on_join
                FROM squadov.user_autoshare_common_settings
                WHERE user_id = $1
                ",
                user_id
            )
                .fetch_one(ex)
                .await?
        )
    }

    pub async fn update_user_auto_share_settings(&self, user_id: i64, data: &AutoShareSetting) -> Result<(), SquadOvError> {
        let mut tx = self.pool.begin().await?;
        self.create_auto_share_settings_for_user_if_not_exist(&mut tx, user_id).await?;

        sqlx::query!(
            "
            UPDATE squadov.user_autoshare_common_settings
            SET share_on_join = $2
            WHERE user_id = $1
            ",
            user_id,
            data.share_on_join,
        )
            .execute(&mut tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn autoshare_to_squad_for_user_on_join(&self, tx: &mut Transaction<'_, Postgres>, user_id: i64, squad_id: i64) -> Result<(), SquadOvError> {
        self.create_auto_share_settings_for_user_if_not_exist(&mut *tx, user_id).await?;

        let settings = self.get_user_auto_share_settings(&mut *tx, user_id).await?;

        if settings.share_on_join {
            // Pull in all the user's VODs and clips and stick them onto RabbitMQ to process for sharing.
            // Since we don't want to do hundreds of database queries of checking the user's permissions, squad permissions, etc.
            let shareable_vods = sqlx::query!(
                r#"
                SELECT v.video_uuid, v.match_uuid, v.is_clip, m.game AS "game!"
                FROM squadov.vods AS v
                INNER JOIN squadov.users AS u
                    ON u.uuid = v.user_uuid
                INNER JOIN squadov.matches AS m
                    ON m.uuid = v.match_uuid
                WHERE u.id = $1
                    AND v.match_uuid IS NOT NULL
                    AND v.user_uuid IS NOT NULL
                    AND v.start_time IS NOT NULL
                    AND v.end_time IS NOT NULL
                "#,
                user_id,
            )
                .fetch_all(&mut *tx)
                .await?;

            for v in shareable_vods {
                match self.sharing_itf.request_vod_share_to_squad(
                    user_id,
                    &v.match_uuid.clone().unwrap(),
                    SquadOvGames::try_from(v.game)?,
                    squad_id,
                    &MatchVideoShareConnection{
                        can_share: true,
                        can_clip: true,
                        id: -1,
                        match_uuid: if v.is_clip {
                            None
                        } else {
                            v.match_uuid.clone()
                        },
                        video_uuid: Some(v.video_uuid.clone()),
                        dest_user_id: None,
                        dest_squad_id: Some(squad_id),
                    },
                    None,
                ).await {
                    Ok(_) => (),
                    Err(err) => log::warn!("Failed to share to squad: {:?}", err)
                };
            }
        }
        Ok(())
    }
}

pub async fn get_user_auto_share_settings_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(HttpResponse::Ok().json(
        &app.get_user_auto_share_settings(&*app.pool, session.user.id).await?
    ))
}

pub async fn edit_user_auto_share_settings_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<AutoShareSetting>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.update_user_auto_share_settings(session.user.id, &data).await?;
    Ok(HttpResponse::NoContent().finish())
}