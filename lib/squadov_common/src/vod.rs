pub mod fastify;
pub mod preview;
pub mod manager;
pub mod db;

use async_trait::async_trait;
use serde::{Serialize,Deserialize};
use sqlx::postgres::{PgPool};
use uuid::Uuid;
use std::str;
use std::clone::Clone;
use crate::{
    SquadOvError,
    SquadOvGames,
    rabbitmq::{
        RABBITMQ_DEFAULT_PRIORITY,
        RABBITMQ_HIGH_PRIORITY,
        RabbitMqInterface,
        RabbitMqListener,
    },
    storage::StorageManager,
};
use std::sync::{Arc};
use tempfile::NamedTempFile;
use chrono::{DateTime, Utc};

const VOD_MAX_AGE_SECONDS: i64 = 21600; // 6 hours

#[derive(Serialize,Deserialize, Clone)]
pub struct VodDestination {
    pub url: String,
    pub bucket: String,
    pub session: String,
}

#[derive(Serialize,Deserialize, Clone)]
pub struct VodAssociation {
    #[serde(rename = "matchUuid")]
    pub match_uuid: Option<Uuid>,
    #[serde(rename = "userUuid")]
    pub user_uuid: Option<Uuid>,
    #[serde(rename = "videoUuid")]
    pub video_uuid: Uuid,
    #[serde(rename = "startTime")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(rename = "endTime")]
    pub end_time: Option<DateTime<Utc>>,
    #[serde(rename = "rawContainerFormat")]
    pub raw_container_format: String,
    #[serde(rename = "isClip")]
    pub is_clip: bool,
    #[serde(rename = "isLocal", default)]
    pub is_local: bool,
}

#[derive(Serialize,Deserialize,Clone,Debug)]
pub struct VodMetadata {
    #[serde(rename = "videoUuid", default)]
    pub video_uuid: Uuid,
    #[serde(rename = "resX")]
    pub res_x: i32,
    #[serde(rename = "resY")]
    pub res_y: i32,
    pub fps: i32,

    #[serde(rename = "minBitrate")]
    pub min_bitrate: i64,
    #[serde(rename = "avgBitrate")]
    pub avg_bitrate: i64,
    #[serde(rename = "maxBitrate")]
    pub max_bitrate: i64,
    pub bucket: String,

    pub id: String,
    #[serde(skip)]
    pub has_fastify: bool,
    #[serde(skip)]
    pub has_preview: bool,
}

impl Default for  VodMetadata {
    fn default() -> Self {
        Self {
            video_uuid: Uuid::new_v4(),
            res_x: 0,
            res_y: 0,
            fps: 0,
            min_bitrate: 0,
            avg_bitrate: 0,
            max_bitrate: 0,
            bucket: String::new(),
            id: String::new(),
            has_fastify: false,
            has_preview: false,
        }
    }
}

#[derive(Deserialize,Debug)]
pub struct VodSegmentId {
    pub video_uuid: Uuid,
    pub quality: String,
    pub segment_name: String
}

impl VodSegmentId {
    fn get_path_parts(&self) -> Vec<String> {
        vec![self.video_uuid.to_string(), self.quality.clone(), self.segment_name.clone()]
    }

    fn get_fname(&self) -> String {
        self.get_path_parts().join("/")
    }
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct VodClip {
    pub clip: VodAssociation,
    pub manifest: VodManifest,
    pub title: String,
    pub description: String,
    pub clipper: String,
    pub game: SquadOvGames,
    pub tm: DateTime<Utc>,
    pub views: i64,
    pub reacts: i64,
    pub comments: i64,
    pub favorite_reason: Option<String>,
    pub is_watchlist: bool,
}

#[derive(Serialize,Deserialize,Clone)]
#[serde(rename_all="camelCase")]
pub struct ClipReact {
}

#[derive(Serialize,Deserialize,Clone)]
#[serde(rename_all="camelCase")]
pub struct ClipComment {
    pub id: i64,
    pub clip_uuid: Uuid,
    pub username: String,
    pub comment: String,
    pub tm: DateTime<Utc>,
}

#[derive(Serialize,Deserialize,Debug)]
pub struct VodSegment {
    pub uri: String,
    pub duration: f32,
    #[serde(rename="segmentStart")]
    pub segment_start: f32,
    #[serde(rename="mimeType")]
    pub mime_type: String,
}

#[derive(Serialize,Deserialize,Debug)]
pub struct VodTrack {
    pub metadata: VodMetadata,
    pub segments: Vec<VodSegment>,
    pub preview: Option<String>,
}

#[derive(Serialize,Deserialize,Debug)]
pub struct VodManifest {
    #[serde(rename="videoTracks")]
    pub video_tracks: Vec<VodTrack>
}

impl Default for VodManifest {
    fn default() -> Self {
        return Self{
            video_tracks: Vec::new()
        }
    }
}

pub struct VodProcessingInterface {
    queue: String,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
    vod: Arc<StorageManager<Arc<dyn manager::VodManager + Send + Sync>>>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum VodProcessingTask {
    Process{
        vod_uuid: Uuid,
        session_id: Option<String>,
        id: Option<String>,
    }
}

#[async_trait]
impl RabbitMqListener for VodProcessingInterface {
    async fn handle(&self, data: &[u8]) -> Result<(), SquadOvError> {
        let task: VodProcessingTask = serde_json::from_slice(data)?;
        match task {
            VodProcessingTask::Process{vod_uuid, id, session_id} => self.process_vod(&vod_uuid, &id.unwrap_or(String::from("source")), session_id.as_ref()).await?, 
        };
        Ok(())
    }
}

impl VodProcessingInterface {
    pub fn new(queue: &str, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>, vod: Arc<StorageManager<Arc<dyn manager::VodManager + Send + Sync>>>) -> Self {
        Self {
            queue: String::from(queue),
            rmq,
            db,
            vod,
        }
    }

    pub async fn request_vod_processing(&self, vod_uuid: &Uuid, id: &str, session_id: Option<String>, high_priority: bool) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.queue, serde_json::to_vec(&VodProcessingTask::Process{
            vod_uuid: vod_uuid.clone(),
            session_id,
            id: Some(id.to_string()),
        })?, if high_priority { RABBITMQ_HIGH_PRIORITY } else { RABBITMQ_DEFAULT_PRIORITY }, VOD_MAX_AGE_SECONDS).await;
        Ok(())
    }

    pub async fn process_vod(&self, vod_uuid: &Uuid, id: &str, session_id: Option<&String>) -> Result<(), SquadOvError> {
        log::info!("Start Processing VOD {} [{:?}]", vod_uuid, session_id);

        log::info!("Get VOD Association");
        let vod = db::get_vod_association(&*self.db, vod_uuid).await?;

        // Need to grab the metadata so we know where this VOD was stored.
        let metadata = db::get_vod_metadata(&*self.db, vod_uuid, id).await?;

        // Grab the appropriate VOD manager. The manager should already exist!
        let manager = self.vod.get_bucket(&metadata.bucket).await.ok_or(SquadOvError::InternalError(format!("Invalid bucket: {}", &metadata.bucket)))?;

        // Note that we can only proceed with "fastifying" the VOD if the entire VOD has been uploaded.
        // We can query GCS's XML API to determine this. If the GCS Session URI is not provided then
        // we assume that the file has already been fully uploaded. If the file hasn't been fully uploaded
        // then we want to defer taking care of this task until later.
        if session_id.is_some() {
            let session_id = session_id.unwrap().clone();
            if !manager.is_vod_session_finished(&session_id).await? {
                log::info!("Defer Fastifying {:?}", vod_uuid);
                return Err(SquadOvError::Defer(1000));
            }
        }

        log::info!("Get Container Extension");
        let raw_extension = container_format_to_extension(&vod.raw_container_format);

        // We do *ALL* processing on the VOD here (for better or worse).
        // 1) Download the VOD to disk using the VOD manager (I think this gets us
        //    faster DL speed than using FFMPEG directly).
        // 2) Convert the video using the vod.fastify module. This gets us a VOD
        //    that has the faststart flag.
        // 3) Generate a preview of the VOD.
        // 4) Upload the processed video and the preview using the VOD manager.
        // 5) Mark the video as being "fastified" (I really need a better word).
        // 6) Mark the video as having a preview.
        log::info!("Generate Input Temp File");
        let input_filename = NamedTempFile::new()?.into_temp_path();
        log::info!("Download VOD - {}", vod_uuid);
        let source_segment_id = VodSegmentId{
            video_uuid: vod_uuid.clone(),
            quality: String::from("source"),
            segment_name: format!("video.{}", &raw_extension),
        };
        manager.download_vod_to_path(&source_segment_id, &input_filename).await?;

        let fastify_filename = NamedTempFile::new()?.into_temp_path();
        let preview_filename = NamedTempFile::new()?.into_temp_path();

        log::info!("Fastify Mp4 - {}", vod_uuid);
        fastify::fastify_mp4(input_filename.as_os_str().to_str().ok_or(SquadOvError::BadRequest)?, &vod.raw_container_format, &fastify_filename).await?;

        log::info!("Generate Preview Mp4 - {}", vod_uuid);
        preview::generate_vod_preview(fastify_filename.as_os_str().to_str().ok_or(SquadOvError::BadRequest)?, &preview_filename).await?;

        log::info!("Upload Fastify VOD - {}", vod_uuid);
        let fastify_segment = VodSegmentId{
            video_uuid: vod_uuid.clone(),
            quality: String::from("source"),
            segment_name: String::from("fastify.mp4"),
        };
        manager.upload_vod_from_file(&fastify_segment, &fastify_filename).await?;

        log::info!("Upload Preview VOD - {}", vod_uuid);
        manager.upload_vod_from_file(&VodSegmentId{
            video_uuid: vod_uuid.clone(),
            quality: String::from("source"),
            segment_name: String::from("preview.mp4"),
        }, &preview_filename).await?;

        log::info!("Process VOD TX (Begin) - {}", vod_uuid);
        let mut tx = self.db.begin().await?;
        log::info!("Mark DB Fastify (Query) - {}", vod_uuid);
        db::mark_vod_as_fastify(&mut tx, vod_uuid).await?;
        log::info!("Mark DB Preview (Query) - {}", vod_uuid);
        db::mark_vod_with_preview(&mut tx, vod_uuid).await?;
        log::info!("Process VOD TX (Commit) - {}", vod_uuid);
        tx.commit().await?;
        log::info!("Delete Source VOD - {}", vod_uuid);
        match manager.delete_vod(&source_segment_id).await {
            Ok(()) => (),
            Err(err) => log::warn!("Failed to delete source VOD: {}", err),
        };

        log::info!("Check if VOD is Public - {}", vod_uuid);
        if db::check_if_vod_public(&*self.db, vod_uuid).await? {
            log::info!("Setting Fastify as Public - {}", vod_uuid);
            manager.make_segment_public(&fastify_segment).await?;
        }

        log::info!("Finish Fastifying {:?}", vod_uuid);
        Ok(())
    }
}

pub fn container_format_to_extension(container_format: &str) -> String {
    match container_format {
        "mpegts" => String::from("ts"),
        _ => String::from("mp4")
    }
}

pub fn container_format_to_mime_type(container_format: &str) -> String {
    match container_format {
        "mpegts" => String::from("video/mp2t"),
        _ => String::from("video/mp4")
    }
}