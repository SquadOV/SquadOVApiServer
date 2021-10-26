use squadov_common::{
    SquadOvError,
    VodMetadata,
    VodSegment,
    VodManifest,
    VodTrack,
    VodDestination,
    VodSegmentId,
    vod::db,
};
use crate::api;
use actix_web::{web, HttpResponse};
use serde::{Deserialize};
use std::default::Default;
use uuid::Uuid;
use std::sync::Arc;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct VodFindFromVideoUuid {
    video_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct VodPartQuery {
    // Should all be set or none be set.
    part: Option<i64>,
    session: Option<String>,
    bucket: Option<String>,
}

impl api::ApiApplication {
    pub async fn get_vod_quality_options(&self, video_uuid: &[Uuid]) -> Result<HashMap<Uuid, Vec<VodMetadata>>, SquadOvError> {
        let metadata = sqlx::query_as!(
            VodMetadata,
            "
            SELECT *
            FROM squadov.vod_metadata
            WHERE video_uuid = ANY($1)
            ",
            video_uuid
        )
            .fetch_all(&*self.pool)
            .await?;

        let mut ret = HashMap::new();
        metadata.into_iter().for_each(|x| {
            if !ret.contains_key(&x.video_uuid) {
                ret.insert(x.video_uuid.clone(), Vec::new());
            }
            let arr = ret.get_mut(&x.video_uuid).unwrap();
            arr.push(x);
        });
        Ok(ret)
    }

    pub async fn get_vod_owner_user_id(&self, video_uuid: &Uuid) -> Result<i64, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT u.id
                FROM squadov.vods AS v
                INNER JOIN squadov.users AS u
                    ON u.uuid = v.user_uuid
                WHERE v.video_uuid = $1
                ",
                video_uuid
            )
                .fetch_one(&*self.pool)
                .await?
                .id
        )
    }

    pub async fn get_vod(&self, video_uuid: &[Uuid]) -> Result<HashMap<Uuid, VodManifest>, SquadOvError> {
        let quality_options = self.get_vod_quality_options(video_uuid).await?;
        let associations = self.find_vod_associations(video_uuid).await?;

        Ok(
            associations.into_iter()
                .filter(|(video_uuid, _assoc)| {
                    quality_options.contains_key(&video_uuid)
                })
                .map(|(video_uuid, assoc)| {
                    // We return our custom manifest format here instead of using M3U8 because we're not
                    // going to be using a standard HLS player anyway and we're going to be using webm+opus
                    // audio files which aren't standard HLS so it doesn't make sense to try and cram our
                    // data into an M3U8 playlist. This way we have more flexibility in playing videos anyway so
                    // all's good in the hood.
                    let mut manifest = VodManifest{
                        ..Default::default()
                    };

                    if let Some(options) = quality_options.get(&video_uuid) {
                        for quality in options {
                            let mut track = VodTrack{
                                metadata: quality.clone(),
                                segments: Vec::new(),
                                preview: None,
                            };

                            let container_format = String::from(if quality.has_fastify {
                                "mp4"
                            } else {
                                &assoc.raw_container_format
                            });

                            // Eventually we'll want to figure out how to do real segments and maintaining
                            // compatability wit Electron but for now just a single file is all we have so just
                            // pretend we just have a single segment.
                            track.segments.push(VodSegment{
                                uri: format!("/v1/vod/{video_uuid}/{quality}/{segment}.{extension}",
                                    video_uuid=video_uuid.clone(),
                                    quality=&quality.id,
                                    segment=if quality.has_fastify {
                                        "fastify"
                                    } else {
                                        "video"
                                    },
                                    extension=&squadov_common::container_format_to_extension(&container_format),
                                ),
                                // Duration is a placeholder - not really needed but will be useful once we get
                                // back to using semgnets.
                                duration: 0.0,
                                segment_start: 0.0,
                                mime_type: squadov_common::container_format_to_mime_type(&container_format),
                            });

                            if quality.has_preview {
                                track.preview = Some(format!(
                                    "/v1/vod/{video_uuid}/{quality}/preview.mp4",
                                    video_uuid=video_uuid.clone(),
                                    quality=&quality.id,
                                ));
                            }

                            manifest.video_tracks.push(track);
                        }
                    }

                    Ok((video_uuid.clone(), manifest))
                })
                .collect::<Result<HashMap<Uuid, VodManifest>, SquadOvError>>()?
        )
    }
}

pub async fn get_vod_handler(data : web::Path<VodFindFromVideoUuid>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let manifest = app.get_vod(&vec![data.video_uuid.clone()]).await?;
    let data = manifest.get(&data.video_uuid).ok_or(SquadOvError::NotFound)?;
    Ok(HttpResponse::Ok().json(data))
}

pub async fn get_vod_upload_path_handler(data : web::Path<VodFindFromVideoUuid>, query: web::Query<VodPartQuery>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let mut assocs = app.find_vod_associations(&[data.video_uuid.clone()]).await?;
    let vod = assocs.remove(&data.video_uuid).ok_or(SquadOvError::NotFound)?;
    Ok(HttpResponse::Ok().json(&
        if let Some(session) = &query.session {
            if let Some(bucket) = &query.bucket {
                let part = query.part.unwrap_or(1);
                if part > 1 {
                    // If we have a session, bucket, and > 1 part, that means we already started the upload so it's a matter
                    // of figuring out the next URL to upload parts to.
                    let manager = app.get_vod_manager(&bucket).await?;
                    let extension = squadov_common::container_format_to_extension(&vod.raw_container_format);
                    VodDestination {
                        url: manager.get_segment_upload_uri(&VodSegmentId{
                            video_uuid: data.video_uuid.clone(),
                            quality: String::from("source"),
                            segment_name: format!("video.{}", extension),
                        }, session, part).await?,
                        bucket: bucket.clone(),
                        session: session.clone(),
                        loc: manager.manager_type(),
                    }
                } else {
                    return Err(SquadOvError::BadRequest);
                }
            } else {
                return Err(SquadOvError::BadRequest);
            }
        } else {
            app.create_vod_destination(&data.video_uuid, &vod.raw_container_format).await?
        }
    ))
}

pub async fn get_vod_association_handler(data : web::Path<VodFindFromVideoUuid>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let mut assocs = app.find_vod_associations(&[data.video_uuid.clone()]).await?;
    Ok(HttpResponse::Ok().json(assocs.remove(&data.video_uuid).ok_or(SquadOvError::NotFound)?))
}

#[derive(Deserialize)]
pub struct VodTrackQuery {
    pub md5: Option<i32>,
}

pub async fn get_vod_track_segment_handler(data : web::Path<squadov_common::VodSegmentId>, app : web::Data<Arc<api::ApiApplication>>, query: web::Query<VodTrackQuery>) -> Result<HttpResponse, SquadOvError> {
    let metadata = db::get_vod_metadata(&*app.pool, &data.video_uuid, "source").await?;
    let manager = app.get_vod_manager(&metadata.bucket).await?;

    let response_string = if let Some(_md5) = query.md5 {
        manager.get_vod_md5(&data).await?
    } else if data.segment_name != "preview.mp4" && db::check_if_vod_public(&*app.pool, &data.video_uuid).await? && manager.check_vod_segment_is_public(&data).await? {
        // If the VOD is public (shared), then we can return the public URL instead of the signed private one.
        manager.get_public_segment_redirect_uri(&data).await?
    } else {
        manager.get_segment_redirect_uri(&data).await?
    };

    // You may be tempted to make this into a TemporaryRedirect and point
    // a media player (e.g. VideoJS) to load from this directly. Don't do that
    // unless you can figure out how to also pass the user's session ID along
    // with that request since this is a protected endpoint.
    Ok(HttpResponse::Ok().json(&response_string))
}