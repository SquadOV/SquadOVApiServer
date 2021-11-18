use squadov_common::{
    SquadOvError,
    vod::{
        VodDestination,
    },
    storage::CloudStorageLocation,
};
use crate::api;
use actix_web::{web, HttpResponse, HttpRequest};
use serde::{Deserialize};
use std::default::Default;
use crate::api::auth::SquadOVSession;
use uuid::Uuid;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SpeedCheckFromUuid {
    file_name_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct SpeedCheckData {
    speed: f64,
}

impl api::ApiApplication {
    async fn update_user_speed_check(&self, user_id: i64, speed_check_speed: &f64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.users
            SET speed_check = $2
            WHERE id = $1
            ",
            user_id,
            speed_check_speed,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_speed_check_destination(&self, file_name_uuid: &Uuid, container_format: &str) -> Result<VodDestination, SquadOvError> {

        let extension = container_format;
        let bucket = self.speed_check.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global storage location configured for VOD storage.")))?;
        let manager = self.get_speed_check_manager(&bucket).await?;
        let session_id = manager.start_speed_check_upload(file_name_uuid).await?;
        let path = manager.get_speed_check_upload_uri(file_name_uuid, &session_id, 1).await?;

        Ok(
            VodDestination{
                url: path,
                bucket,
                session: session_id,
                loc: manager.manager_type(),
            }
        )
    }
}

pub async fn update_user_speed_check_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<SpeedCheckData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.update_user_speed_check(session.user.id, &data.speed).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_upload_speed_check_path_handler(data : web::Path<SpeedCheckFromUuid>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(&
        app.create_speed_check_destination(&data.file_name_uuid, "JPG").await?
    ))
}