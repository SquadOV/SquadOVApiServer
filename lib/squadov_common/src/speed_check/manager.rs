pub mod aws_speed_check_manager;

pub use aws_speed_check_manager::*;

use async_trait::async_trait;
use crate::{
    SquadOvError,
    vod::manager::UploadManagerType,
    vod::manager::UploadPurpose,
};
use uuid::Uuid;

#[async_trait]
pub trait SpeedCheckManager {
    fn manager_type(&self) -> UploadManagerType;
    fn upload_purpose(&self) -> UploadPurpose {
        UploadPurpose::SpeedCheck
    }

    // Returns a session string to start the speed check upload
    async fn start_speed_check_upload(&self, file_name_uuid: &Uuid) -> Result<String, SquadOvError>;
    // This gets the next part to upload
    async fn get_speed_check_upload_uri(&self, file_name_uuid: &Uuid, session_id: &str, part: i64) -> Result<String, SquadOvError>;
}