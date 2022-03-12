use squadov_common::{
    SquadOvError,
    vod::db,
};
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use std::sync::Arc;
use serde::{Deserialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct VodDeleteFromUuid {
    video_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct BulkDeleteVodData {
    vods: Vec<Uuid>,
}

impl api::ApiApplication {

    async fn filter_deletable_vods(&self, uuids: &[Uuid], user_id: i64) -> Result<Vec<Uuid>, SquadOvError> {
        // Users can only delete their own VODs.
        // We need to check this manually since the API isn't able to do more complex checks
        // on which clips the user selected.
        Ok(
            sqlx::query!(
                "
                SELECT v.video_uuid
                FROM squadov.vods AS v
                INNER JOIN squadov.users AS u
                    ON u.uuid = v.user_uuid
                WHERE v.video_uuid = ANY($1)
                    AND u.id = $2
                ",
                uuids,
                user_id
            )
                .fetch_all(&*self.pool)
                .await?
                .into_iter()
                .map(|x| { x.video_uuid })
                .collect()
        )
    }

    async fn bulk_delete_vods_database(&self, uuids: &[Uuid]) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            DELETE FROM squadov.vods
            WHERE video_uuid = ANY($1)
            ",
            uuids
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    async fn bulk_delete_vods(&self, uuids: &[Uuid], user_id: i64) -> Result<(), SquadOvError> {
        let uuids = self.filter_deletable_vods(uuids, user_id).await?;
        if uuids.is_empty() {
            return Ok(());
        }

        let quality_options = self.get_vod_quality_options(&uuids).await?;
        let assocs = self.find_vod_associations(&uuids).await?;
        let metadata = db::get_bulk_vod_metadata(&*self.pool, &uuids.iter().map(|x| {
            ( x.clone(), "source")
        }).collect::<Vec<(Uuid, &str)>>()).await?;
        self.bulk_delete_vods_database(&uuids).await?;

        for u in &uuids {
            if let Some(quality_arr) = quality_options.get(u) {
                if let Some(vod) = assocs.get(u) {
                    if let Some(metadata) = metadata.get(&(u.clone(), String::from("source"))) {
                        let manager = self.get_vod_manager(&metadata.bucket).await?;

                        let raw_extension = squadov_common::container_format_to_extension(&vod.raw_container_format);
                        for quality in quality_arr {
                            let _ = manager.delete_vod(&squadov_common::VodSegmentId{
                                video_uuid: u.clone(),
                                quality: quality.id.clone(),
                                segment_name: format!("video.{}", &raw_extension),
                            }).await;
                
                            if quality.has_fastify {
                                let _ = manager.delete_vod(&squadov_common::VodSegmentId{
                                    video_uuid: u.clone(),
                                    quality: quality.id.clone(),
                                    segment_name: String::from("fastify.mp4"),
                                }).await;
                            }
                
                            if quality.has_preview {
                                let _ = manager.delete_vod(&squadov_common::VodSegmentId{
                                    video_uuid: u.clone(),
                                    quality: quality.id.clone(),
                                    segment_name: String::from("preview.mp4"),
                                }).await;
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
}

pub async fn delete_vod_handler(data : web::Path<VodDeleteFromUuid>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.bulk_delete_vods(&[data.video_uuid.clone()], session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn bulk_delete_vods_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<BulkDeleteVodData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.bulk_delete_vods(&data.vods, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}