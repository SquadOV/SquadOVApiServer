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
use std::io::BufReader;
use tempfile::NamedTempFile;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

const VOD_MAX_AGE_SECONDS: i64 = 21600; // 6 hours

#[derive(Serialize,Deserialize, Clone)]
pub struct VodDestination {
    pub url: String,
    pub bucket: String,
    pub session: String,
    pub loc: manager::UploadManagerType,
    pub purpose: manager::UploadPurpose,
}

#[derive(Serialize,Deserialize, Clone)]
pub struct VodThumbnail {
    pub video_uuid: Uuid,
    pub bucket: String,
    pub filepath: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize,Deserialize, Clone, Debug)]
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
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,

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
            session_id: None,
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

#[derive(Deserialize, Debug)]
pub struct RawVodTag {
    pub video_uuid: Uuid,
    pub tag_id: i64,
    pub tag: String,
    pub user_id: i64,
    pub tm: DateTime<Utc>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all="camelCase")]
pub struct VodTag {
    pub video_uuid: Uuid,
    // The text of the tag
    pub tag: String,
    pub tag_id: i64,
    // How many people applied this same exact tag
    pub count: i64,
    // Whether or not the person doing the query applied this tag
    pub is_self: bool,
}

pub fn condense_raw_vod_tags(tags: Vec<RawVodTag>, self_user_id: i64) -> Result<Vec<VodTag>, SquadOvError> {
    let mut store: HashMap<String, VodTag> = HashMap::new();
    for t in tags {
        if !store.contains_key(&t.tag) {
            store.insert(t.tag.clone(), VodTag{
                video_uuid: t.video_uuid.clone(),
                tag: t.tag.clone(),
                tag_id: t.tag_id,
                count: 0,
                is_self: false,
            });
        }

        if let Some(mt) = store.get_mut(&t.tag) {
            mt.count += 1;
            mt.is_self |= t.user_id == self_user_id;
        }
    }
    Ok(store.values().cloned().collect())
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
    pub access_token: Option<String>,
    pub tags: Vec<VodTag>,
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
        log::info!("Handle VOD Task: {}", std::str::from_utf8(data).unwrap_or("failure"));
        let task: VodProcessingTask = serde_json::from_slice(data)?;
        match task {
            VodProcessingTask::Process{vod_uuid, id, session_id} => self.process_vod(
                &vod_uuid,
                &id.unwrap_or(String::from("source")),
                session_id.as_ref(),
            ).await?, 
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

        log::info!("Get Container Extension");
        let raw_extension = container_format_to_extension(&vod.raw_container_format);
        let source_segment_id = VodSegmentId{
            video_uuid: vod_uuid.clone(),
            quality: String::from("source"),
            segment_name: format!("video.{}", &raw_extension),
        };

        // Need to grab the metadata so we know where this VOD was stored.
        log::info!("Get VOD Metadata");
        let metadata = db::get_vod_metadata(&*self.db, vod_uuid, id).await?;

        // Grab the appropriate VOD manager. The manager should already exist!
        log::info!("Get VOD Manager");
        let manager = self.vod.get_bucket(&metadata.bucket).await.ok_or(SquadOvError::InternalError(format!("Invalid bucket: {}", &metadata.bucket)))?;

        // Note that we can only proceed with "fastifying" the VOD if the entire VOD has been uploaded.
        // We can query GCS's XML API to determine this. If the GCS Session URI is not provided then
        // we assume that the file has already been fully uploaded. If the file hasn't been fully uploaded
        // then we want to defer taking care of this task until later.
        if let Some(session) = session_id {
            log::info!("Checking Segment Upload Finished");
            if !manager.is_vod_session_finished(&session).await? {
                log::info!("Defer Fastifying {:?}", vod_uuid);
                return Err(SquadOvError::Defer(1000));
            }
        } 

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
        
        manager.download_vod_to_path(&source_segment_id, &input_filename).await?;

        let fastify_filename = NamedTempFile::new()?.into_temp_path();
        let preview_filename = NamedTempFile::new()?.into_temp_path();
        let thumbnail_filename = NamedTempFile::new()?.into_temp_path();

        log::info!("Fastify Mp4 - {}", vod_uuid);
        fastify::fastify_mp4(input_filename.as_os_str().to_str().ok_or(SquadOvError::BadRequest)?, &vod.raw_container_format, &fastify_filename).await?;

        // Get VOD length in seconds - we use this to manually determine where to clip.
        let length_seconds = vod.end_time.unwrap_or(Utc::now()).signed_duration_since(vod.start_time.unwrap_or(Utc::now())).num_seconds();

        log::info!("Generate Preview Mp4 - {}", vod_uuid);
        preview::generate_vod_preview(fastify_filename.as_os_str().to_str().ok_or(SquadOvError::BadRequest)?, &preview_filename, length_seconds).await?;

        log::info!("Generate Thumbnail - {}", vod_uuid);
        preview::generate_vod_thumbnail(fastify_filename.as_os_str().to_str().ok_or(SquadOvError::BadRequest)?, &thumbnail_filename, length_seconds).await?;

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

        log::info!("Upload Thumbnail - {}", vod_uuid);
        let thumbnail_id = VodSegmentId{
            video_uuid: vod_uuid.clone(),
            quality: String::from("source"),
            segment_name: String::from("thumbnail.jpg"),
        };
        manager.upload_vod_from_file(&thumbnail_id, &thumbnail_filename).await?;

        log::info!("Process VOD TX (Begin) - {}", vod_uuid);
        let mut tx = self.db.begin().await?;
        log::info!("Mark DB Fastify (Query) - {}", vod_uuid);
        db::mark_vod_as_fastify(&mut tx, vod_uuid).await?;
        log::info!("Mark DB Preview (Query) - {}", vod_uuid);
        db::mark_vod_with_preview(&mut tx, vod_uuid).await?;
        log::info!("Add DB Thumbnail (Query) - {}", vod_uuid);
        {
            let file = std::fs::File::open(&thumbnail_filename)?;
            let image = image::io::Reader::with_format(BufReader::new(file), image::ImageFormat::Jpeg);
            let thumbnail_dims = image.into_dimensions()?;
            db::add_vod_thumbnail(&mut tx, vod_uuid, &metadata.bucket, &thumbnail_id, thumbnail_dims.0 as i32, thumbnail_dims.1 as i32).await?;
        }

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

            log::info!("Setting Thumbnail as Public - {}", vod_uuid);
            manager.make_segment_public(&thumbnail_id).await?;
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