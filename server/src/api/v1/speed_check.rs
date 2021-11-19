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
    speed_mbps: f64,
}

impl api::ApiApplication {
    async fn update_user_speed_check(&self, user_id: i64, speed_check_speed_mbps: f64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.users
            SET speed_check = $2
            WHERE id = $1
            ",
            user_id,
            speed_check_speed_mbps,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn create_speed_check_destination(&self, file_name_uuid: &Uuid) -> Result<VodDestination, SquadOvError> {
        let bucket = self.speed_check.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global storage location configured for Speed Check storage.")))?;
        let manager = self.get_speed_check_manager(&bucket).await?;
        let session_id = manager.start_speed_check_upload(file_name_uuid).await?;
        let path = manager.get_speed_check_upload_uri(file_name_uuid, &session_id).await?;

        Ok(
            VodDestination{
                url: path,
                bucket,
                session: session_id,
                loc: manager.manager_type(),
            }
        )
    }

    async fn clean_up_speed_check_on_cloud(&self, file_name_uuid: &Uuid) -> Result<(), SquadOvError> {
        let bucket = self.speed_check.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global storage location configured for Speed Check storage.")))?;
        let manager = self.get_speed_check_manager(&bucket).await?;
        manager.delete_speed_check(file_name_uuid).await?;
        Ok(())
    }
}

pub async fn update_user_speed_check_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<SpeedCheckData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.update_user_speed_check(session.user.id, data.speed_mbps).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn clean_up_speed_check_on_cloud_handler(data : web::Path<SpeedCheckFromUuid>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    app.clean_up_speed_check_on_cloud(&data.file_name_uuid).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_upload_speed_check_path_handler(data : web::Path<SpeedCheckFromUuid>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(&
        app.create_speed_check_destination(&data.file_name_uuid).await?
    ))
}