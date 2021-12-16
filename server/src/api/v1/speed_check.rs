use squadov_common::{
    SquadOvError,
    vod::{
        VodDestination,
    },
    storage::CloudStorageLocation,
};
use crate::api;
use actix_web::{web, HttpResponse, HttpRequest};
use serde::{Deserialize, Serialize};
use api::v1::UploadPartQuery;
use std::default::Default;
use crate::api::auth::SquadOVSession;
use uuid::Uuid;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SpeedCheckFromUuid {
    file_name_uuid: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all="camelCase")]
pub struct SpeedCheckData {
    speed_mbps: f64,
}

impl api::ApiApplication {
    async fn update_user_speed_check(&self, user_id: i64, speed_check_speed_mbps: f64) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.users
            SET speed_check_mbps = $2
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
        let path = manager.get_speed_check_upload_uri(file_name_uuid, &session_id, 1).await?;

        Ok(
            VodDestination{
                url: path,
                bucket,
                session: session_id,
                loc: manager.manager_type(),
                purpose: manager.upload_purpose(),
            }
        )
    }
}

pub async fn update_user_speed_check_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<SpeedCheckData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    app.update_user_speed_check(session.user.id, data.speed_mbps).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_upload_speed_check_path_handler(data : web::Path<SpeedCheckFromUuid>, query: web::Query<UploadPartQuery>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    Ok(HttpResponse::Ok().json(&
        if let Some(session) = &query.session {
            if let Some(bucket) = &query.bucket {
                let part = query.part.unwrap_or(1);
                if part > 1 {
                    // If we have a session, bucket, and > 1 part, that means we already started the upload so it's a matter
                    // of figuring out the next URL to upload parts to.
                    let manager = app.get_speed_check_manager(&bucket).await?;
                    VodDestination {
                        url: manager.get_speed_check_upload_uri(&data.file_name_uuid, session, part).await?,
                        bucket: bucket.clone(),
                        session: session.clone(),
                        loc: manager.manager_type(),
                        purpose: manager.upload_purpose(),
                    }
                } else {
                    return Err(SquadOvError::BadRequest);
                }
            } else {
                return Err(SquadOvError::BadRequest);
            }
        } else {
            app.create_speed_check_destination(&data.file_name_uuid).await?
        }
    ))
}