pub mod aws_speed_check_manager;

pub use aws_speed_check_manager::*;

use async_trait::async_trait;
use crate::{
    SquadOvError,
    vod::manager::VodManagerType,
};
use serde_repr::{Serialize_repr, Deserialize_repr};
use uuid::Uuid;

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug)]
#[repr(i32)]
pub enum SpeedCheckManagerType {
    FileSystem,
    GCS,
    S3,
}

pub fn get_speed_check_manager_type(root: &str) -> VodManagerType {
    if root.starts_with("gs://") {
        VodManagerType::GCS
    } else if root.starts_with("s3://") {
        VodManagerType::S3
    } else {
        VodManagerType::FileSystem
    }
}

#[async_trait]
pub trait SpeedCheckManager {
    fn manager_type(&self) -> VodManagerType;

    // Returns a session string that can be passed to get_segment_upload_uri
    async fn start_speed_check_upload(&self, file_name_uuid: &Uuid) -> Result<String, SquadOvError>;
    // User can request to get a separate URL for each uploaded segment (though it isn't necessarily guaranteed to be different for each segment).
    async fn get_speed_check_upload_uri(&self, file_name_uuid: &Uuid, session_id: &str, part: i64) -> Result<String, SquadOvError>;
    // At the end, the user may need to finish the segment upload by giving us the session id as well as a list of parts that were uploaded.
    async fn finish_speed_check_upload(&self, file_name_uuid: &Uuid, session_id: &str, parts: &[String]) -> Result<(), SquadOvError>;
}