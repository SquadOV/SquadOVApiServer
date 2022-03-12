use squadov_common::SquadOvError;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use std::sync::Arc;
use crate::api::{
    self,
    auth::SquadOVSession,
    v1::VodFindFromVideoUuid,
};
use serde::Deserialize;
use sqlx::{Transaction, Postgres};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ReferralQuery {
    #[serde(rename="ref")]
    pub referral_code: Option<String>,
}

#[derive(Deserialize)]
pub struct VodWatchRangeData {
    pub start: f64,
    pub end: f64,
}

impl api::ApiApplication {
    pub async fn record_user_event(&self, user_ids: &[i64], event: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.user_event_record (
                user_id,
                event_name,
                tm
            )
            SELECT inp.id, $2, NOW()
            FROM UNNEST($1::BIGINT[]) AS inp(id)
            ON CONFLICT DO NOTHING
            ",
            user_ids,
            event,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn regenerate_user_referral_code(&self, tx: &mut Transaction<'_, Postgres>, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.referral_codes
            SET code = LOWER(sub.username)
            FROM (
                SELECT username
                FROM squadov.users
                WHERE id = $1
            ) AS sub
            WHERE user_id = $1
            ",
            user_id,
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    pub async fn get_user_referral_code(&self, user_id: i64) -> Result<Option<String>, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                SELECT code AS "code!"
                FROM squadov.referral_codes
                WHERE user_id = $1
                "#,
                user_id,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    x.code
                })
        )
    }

    pub async fn create_referral_code(&self, code: &str, user_id: Option<i64>) -> Result<String, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                INSERT INTO squadov.referral_codes (
                    id,
                    code,
                    description,
                    user_id,
                    tm
                )
                VALUES (
                    gen_random_uuid(),
                    $1,
                    '',
                    $2,
                    NOW()
                )
                RETURNING code
                ",
                code.to_lowercase(),
                user_id,
            )
                .fetch_one(&*self.pool)
                .await?
                .code
        )
    }

    pub async fn mark_referral_visit(&self, code: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.referral_visits (
                code,
                tm
            )
            SELECT c.id, NOW()
            FROM squadov.referral_codes AS c
            WHERE c.code = $1
            ",
            code,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_referral_download(&self, code: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.referral_downloads (
                code,
                tm
            )
            SELECT c.id, NOW()
            FROM squadov.referral_codes AS c
            WHERE c.code = $1
            ",
            code,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_user_download(&self, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.user_downloads (
                user_id,
                tm
            )
            VALUES (
                $1,
                NOW()
            )
            ",
            user_id
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_user_vod_watch_analytics(&self, video_uuid: &Uuid, user_id: i64, start: f64, end: f64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.vod_watch_analytics (
                video_uuid,
                user_id,
                start_seconds,
                end_seconds,
                tm
            ) VALUES (
                $1,
                $2,
                $3,
                $4,
                NOW()
            )
            ",
            video_uuid,
            user_id,
            start,
            end,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }
}

pub async fn public_landing_visit_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<ReferralQuery>) -> Result<HttpResponse, SquadOvError> {
    if query.referral_code.is_some() {
        match app.mark_referral_visit(query.referral_code.as_ref().unwrap()).await {
            Ok(_) => (),
            Err(err) => log::warn!("Failed to mark referral visit: {:?}", err)
        };
    }
    Ok(HttpResponse::NoContent().finish())
}

pub async fn mark_user_download_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let opt_code = app.get_referral_code_associated_to_user(session.user.id).await?;
    if let Some(code) = opt_code {
        match app.mark_referral_download(&code).await {
            Ok(_) => (),
            Err(err) => log::warn!("Failed to mark referral download: {:?}", err)
        };
    }

    match app.mark_user_download(session.user.id).await {
        Ok(_) => (),
        Err(err) => log::warn!("Failed to mark user download: {:?}", err)
    };

    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_user_me_referral_link_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let mut code = app.get_user_referral_code(session.user.id).await?;
    if code.is_none() {
        code = Some(app.create_referral_code(&session.user.username, Some(session.user.id)).await?);
    }

    let code = code.unwrap();
    Ok(HttpResponse::Ok().json(format!(
        "{}/?ref={}",
        &app.config.squadov.landing_url,
        code
    )))
}

pub async fn create_user_vod_watch_analytics_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<VodFindFromVideoUuid>, data: web::Json<VodWatchRangeData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.add_user_vod_watch_analytics(&path.video_uuid, session.user.id, data.start, data.end).await?;
    Ok(HttpResponse::NoContent().finish())
}