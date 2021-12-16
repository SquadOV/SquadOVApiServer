pub mod filesystem_vod_manager;
pub mod gcs_vod_manager;
pub mod aws_vod_manager;

pub use filesystem_vod_manager::*;
pub use gcs_vod_manager::*;
pub use aws_vod_manager::*;

use async_trait::async_trait;
use crate::{
    SquadOvError,
    VodSegmentId,
};
use serde_repr::{Serialize_repr, Deserialize_repr};
use chrono::{DateTime, Utc};

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug)]
#[repr(i32)]
pub enum UploadManagerType {
    FileSystem,
    GCS,
    S3,
}

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug)]
#[repr(i32)]
pub enum UploadPurpose {
    VOD,
    SpeedCheck,
}

pub fn get_upload_manager_type(root: &str) -> UploadManagerType {
    if root.starts_with("gs://") {
        UploadManagerType::GCS
    } else if root.starts_with("s3://") {
        UploadManagerType::S3
    } else {
        UploadManagerType::FileSystem
    }
}

#[async_trait]
pub trait VodManager {
    fn manager_type(&self) -> UploadManagerType;
    fn upload_purpose(&self) -> UploadPurpose {
        UploadPurpose::VOD
    }

    // Returns a session string that can be passed to get_segment_upload_uri
    async fn start_segment_upload(&self, segment: &VodSegmentId) -> Result<String, SquadOvError>;
    // User can request to get a separate URL for each uploaded segment (though it isn't necessarily guaranteed to be different for each segment).
    async fn get_segment_upload_uri(&self, segment: &VodSegmentId, session_id: &str, part: i64) -> Result<String, SquadOvError>;
    // At the end, the user may need to finish the segment upload by giving us the session id as well as a list of parts that were uploaded.
    async fn finish_segment_upload(&self, segment: &VodSegmentId, session_id: &str, parts: &[String]) -> Result<(), SquadOvError>;

    async fn download_vod_to_path(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError>;
    async fn upload_vod_from_file(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError>;
    async fn is_vod_session_finished(&self, session: &str) -> Result<bool, SquadOvError>;
    async fn get_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<(String, Option<DateTime<Utc>>), SquadOvError>;
    async fn get_public_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError>;
    async fn make_segment_public(&self, segment: &VodSegmentId) -> Result<(), SquadOvError>;
    async fn check_vod_segment_is_public(&self, segment: &VodSegmentId) -> Result<bool, SquadOvError>;
    async fn delete_vod(&self, segment: &VodSegmentId) -> Result<(), SquadOvError>;
    async fn get_vod_md5(&self, segment: &VodSegmentId) -> Result<String, SquadOvError>;
}