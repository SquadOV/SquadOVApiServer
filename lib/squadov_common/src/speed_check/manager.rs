pub mod aws_speed_check_manager;

pub use aws_speed_check_manager::*;

use async_trait::async_trait;
use crate::{
    SquadOvError,
    vod::manager::VodManagerType,
};
use serde_repr::{Serialize_repr, Deserialize_repr};
use uuid::Uuid;

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

    // Returns a session string to start the speed check upload
    async fn start_speed_check_upload(&self, file_name_uuid: &Uuid) -> Result<String, SquadOvError>;
    // This gets the next part to upload
    async fn get_speed_check_upload_uri(&self, file_name_uuid: &Uuid, session_id: &str, part: i64) -> Result<String, SquadOvError>;
    // Not currently used, but functionality to delete the speed_check if it ever finished (probably could delete this actually)
    async fn delete_speed_check(&self, file_name_uuid: &Uuid) -> Result<(), SquadOvError>; 
}