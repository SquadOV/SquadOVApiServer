use crate::api;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api::auth::{SquadOvMachineId, SquadOVSession};
use std::sync::Arc;
use serde::{Deserialize};
use uuid::Uuid;
use sqlx::{Transaction, Postgres};
use squadov_common::{
    SquadOvError,
    vod::{
        VodAssociation,
        VodDestination,
        VodSegmentId,
        VodCopyLocation,
        container_format_to_extension,
        db as vdb,
        manager::StorageType,
    },
    storage::CloudStorageLocation,
    rabbitmq::RABBITMQ_DEFAULT_PRIORITY,
};

#[derive(Deserialize)]
pub struct VodCreateDestinationUriInput {
    #[serde(rename="videoUuid")]
    video_uuid: Uuid,
    #[serde(rename="containerFormat")]
    container_format: String,
}

#[derive(Deserialize)]
pub struct VodAssociatePathInput {
    video_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct VodAssociateBodyInput {
    association: VodAssociation,
    metadata: squadov_common::VodMetadata,
    #[serde(rename="sessionUri")]
    session_uri: Option<String>,
    parts: Option<Vec<String>>,
}

impl api::ApiApplication {
    pub async fn create_vod_destination(&self, video_uuid: &Uuid, container_format: &str, use_accel: bool) -> Result<VodDestination, SquadOvError> {
        let extension = squadov_common::container_format_to_extension(container_format);

        let bucket = self.vod.get_bucket_for_location(CloudStorageLocation::Global).ok_or(SquadOvError::InternalError(String::from("No global storage location configured for VOD storage.")))?;
        let manager = self.get_vod_manager(&bucket).await?;

        let vod_segment = squadov_common::VodSegmentId{
            video_uuid: video_uuid.clone(),
            quality: String::from("source"),
            segment_name: format!("video.{}", &extension),
        };
        let session_id = manager.start_segment_upload(&vod_segment, StorageType::Hot).await?;
        let path = manager.get_segment_upload_uri(&vod_segment, &session_id, 1, use_accel).await?;
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

    pub async fn update_vod_metadata_session_id(&self, tx : &mut Transaction<'_, Postgres>, video_uuid: &Uuid, metadata_id: &str, session_id: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            UPDATE squadov.vod_metadata
            SET session_id = $3
            WHERE video_uuid = $1
                AND id = $2
            ",
            video_uuid,
            metadata_id,
            session_id,
        )
            .execute(tx)
            .await?;
        Ok(())
    }
}

pub async fn associate_vod_handler(path: web::Path<VodAssociatePathInput>, data : web::Json<super::VodAssociateBodyInput>, app : web::Data<Arc<api::ApiApplication>>, machine_id: Option<web::Header<SquadOvMachineId>>, request : HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let data = data.into_inner();
    if path.video_uuid != data.association.video_uuid {
        return Err(SquadOvError::BadRequest);
    }

    // If the current user doesn't match the UUID passed in the association then reject the request.
    // We could potentially force the association to contain the correct user UUID but in reality
    // the user *should* know their own user UUID.
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    if data.association.is_clip {
        if let Some(et) = data.association.end_time.as_ref() {
            if let Some(st) = data.association.start_time.as_ref() {
                // Check max clip length here
                if ((et.clone() - st.clone()).num_milliseconds() - 4000) >= (session.features.as_ref().ok_or(SquadOvError::Unauthorized)?.max_clip_seconds * 1000) {
                    return Err(SquadOvError::BadRequest);
                }
            }
        }
    }
    
    let input_user_uuid = match data.association.user_uuid {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    if input_user_uuid != session.user.uuid {
        return Err(SquadOvError::Unauthorized);
    }

    let mut tx = app.pool.begin().await?;
    vdb::associate_vod(&mut tx, &data.association).await?;

    let metadata_id = data.metadata.id.clone();
    let bucket = data.metadata.bucket.clone();
    if !data.association.is_local {
        vdb::bulk_add_video_metadata(&mut tx, &data.association.video_uuid, &[data.metadata]).await?;
    }

    // Need to store the session id for the VOD upload just in case we need it later on.
    if let Some(session_uri) = &data.session_uri {
        app.update_vod_metadata_session_id(&mut tx, &data.association.video_uuid, &metadata_id, &session_uri).await?;
    }

    // Once the VOD is finished - we need to take care of who we actually want to share the match/VOD/clip with.
    if !data.association.is_local {
        app.handle_vod_share(&mut tx, session.user.id, &data.association).await?;
    }

    // Upon association, the video *should* only exist in one place. Either on the cloud OR on the user's machine.
    if data.association.is_local {
        if machine_id.is_none() {
            return Err(SquadOvError::BadRequest);
        }
        vdb::bulk_sync_vod_copies(&mut tx, &[data.association.video_uuid.clone()], VodCopyLocation::Local, machine_id.map(|x| { x.id.clone() }).unwrap_or(String::new()).as_str()).await?;
    } else {
        vdb::bulk_sync_vod_copies(&mut tx, &[data.association.video_uuid.clone()], VodCopyLocation::Cloud, &bucket).await?;
    }

    tx.commit().await?;

    // At this point the VOD/clip should be ready for an initial sync to ES.
    app.es_itf.request_sync_vod(vec![data.association.video_uuid.clone()]).await?;

    // Note that we don't want to spawn a task directly here to "fastify" the VOD
    // because it does take a significant amount of memory/disk space to do so.
    // So we toss it to the local job queue so we can better limit the amount of resources we end up using.
    if !data.association.is_local {
        if let Some(session_uri) = data.session_uri.as_ref() {
            let manager = app.get_vod_manager(&bucket).await?;
            let raw_extension = container_format_to_extension(&data.association.raw_container_format);
            // Need to finish the VOD upload here as well, while this could theoretically take a bit, I think in practice
            // it generally finishes pretty fast. We can't do the 'finish' in the VOD processing because there's certain
            // situations where we'll need to VOD to be uploaded BEFORE it gets to the VOD processing. This is the case
            // in VOD clipping where we'll request public access on the raw uploaded clip before it hits the VOD processing
            // which would result in a 403 since the multi-part uploaded clip doesn't exist yet.
            manager.finish_segment_upload(&VodSegmentId{
                video_uuid: data.association.video_uuid.clone(),
                quality: String::from("source"),
                segment_name: format!("video.{}", &raw_extension),
            }, session_uri, &data.parts.unwrap_or(vec![])).await?;
        }

        app.vod_itf.request_vod_processing(
            &data.association.video_uuid,
            &metadata_id,
            data.session_uri.clone(),
            session.features.as_ref().map(|x| { x.vod_priority as u8 }).unwrap_or(RABBITMQ_DEFAULT_PRIORITY),
        ).await?;

        // If this is the user's first VOD, we want to record that in our analytics so that we can tell users about their momentous occasation.
        if !data.association.is_clip && app.get_user_full_match_vod_count(session.user.id).await? == 1 {
            let event = "recordfirst";
            app.segment.track(&session.user.uuid.to_string(), event).await?;
            app.record_user_event(&[session.user.id], event, Some("DESKTOP")).await?;
        }
    }
    Ok(HttpResponse::Ok().finish())
}

pub async fn create_vod_destination_handler(data : web::Json<VodCreateDestinationUriInput>, app : web::Data<Arc<api::ApiApplication>>, request: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    // First we need to make sure this vod UUID is available in the database before
    // giving the user a path to upload the file.
    let extensions = request.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };
    
    let mut tx = app.pool.begin().await?;
    vdb::reserve_vod_uuid(&mut tx, &data.video_uuid, &data.container_format, session.user.id, false).await?;
    tx.commit().await?;

    Ok(HttpResponse::Ok().json(app.create_vod_destination(&data.video_uuid, &data.container_format, false).await?))
}