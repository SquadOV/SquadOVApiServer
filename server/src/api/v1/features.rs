use serde::Serialize;
use squadov_common::{SquadOvError};
use actix_web::{web, HttpResponse};
use crate::api;
use std::sync::Arc;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    user_id: i64,
    max_record_pixel_y: i32,
    max_record_fps: i32,
    allow_record_upload: bool,
    allow_wow_combat_log_upload: bool,
    enable_user_profiles: bool,
    disable_sentry: bool,
    max_bitrate_kbps: i32,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            user_id: -1,
            max_record_pixel_y: 1080,
            max_record_fps: 60,
            allow_record_upload: true,
            allow_wow_combat_log_upload: true,
            enable_user_profiles: true,
            disable_sentry: false,
            max_bitrate_kbps: 9000,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalFlags {
    disable_registration: bool,
}

impl Default for GlobalFlags {
    fn default() -> Self {
        Self {
            disable_registration: false,
        }
    }
}

impl api::ApiApplication {
    pub async fn get_global_app_flags(&self) -> Result<GlobalFlags, SquadOvError> {
        let kvp_flags = sqlx::query!("
            SELECT *
            FROM squadov.global_app_flags
        ")
            .fetch_all(&*self.pool)
            .await?;

        let mut flags = GlobalFlags::default();
        for kvp in kvp_flags {
            match kvp.fkey.to_lowercase().as_str() {
                "disable_registration" => flags.disable_registration = kvp.fvalue.parse::<bool>()?,
                _ => (),
            };
        }
        Ok(flags)
    }
}

pub async fn get_user_feature_flags_handler(app : web::Data<Arc<api::ApiApplication>>, data : web::Path<super::UserResourcePath>) -> Result<HttpResponse, SquadOvError> {
    let flags = sqlx::query_as!(
        FeatureFlags,
        "
        SELECT *
        FROM squadov.user_feature_flags
        WHERE user_id = $1
        ",
        data.user_id,
    )
        .fetch_optional(&*app.pool)
        .await?;

    if flags.is_some() {
        Ok(HttpResponse::Ok().json(&flags.unwrap()))
    } else {
        Ok(HttpResponse::Ok().json(FeatureFlags{
            user_id: data.user_id,
            ..FeatureFlags::default()
        }))
    }
}

pub async fn get_global_app_flags_handler(app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(app.get_global_app_flags().await?))
}