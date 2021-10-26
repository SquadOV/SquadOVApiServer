use crate::{
    SquadOvError,
    vod::manager::VodManager,
    VodSegmentId
};
use async_trait::async_trait;
use std::path::{Path,PathBuf};
use std::{fs, io};
use md5::{Md5, Digest};

pub struct FilesystemVodManager {
    root: String
}

impl FilesystemVodManager {
    pub fn new(root: &str) -> Result<FilesystemVodManager, SquadOvError> {
        let manager = FilesystemVodManager{
            root: root.to_string(),
        };

        // The root folder must exist.
        if !Path::new(&manager.root).exists() {
            return Err(SquadOvError::NotFound);
        }

        Ok(manager)
    }
}

impl FilesystemVodManager {
    fn segment_id_to_path(&self, segment: &VodSegmentId) -> PathBuf {
        Path::new(&self.root).join(&segment.video_uuid.to_string()).join(&segment.quality).join(&segment.segment_name)
    }
}

#[async_trait]
impl VodManager for FilesystemVodManager {
    fn manager_type(&self) -> super::VodManagerType {
        super::VodManagerType::FileSystem
    }

    async fn get_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        let fname = self.segment_id_to_path(segment);
        if !fname.exists() {
            return Err(SquadOvError::NotFound);
        }

        Ok(String::from(fname.to_str().unwrap_or("")))
    }

    async fn download_vod_to_path(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        let fname = self.segment_id_to_path(segment);
        std::fs::copy(&fname, path)?;
        Ok(())
    }

    async fn is_vod_session_finished(&self, _session: &str) -> Result<bool, SquadOvError> {
        Ok(true)
    }

    async fn upload_vod_from_file(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        let fname = self.segment_id_to_path(segment);
        std::fs::copy(path, &fname)?;
        Ok(())
    }

    async fn start_segment_upload(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        Ok(String::from(self.segment_id_to_path(segment).to_str().unwrap_or("")))
    }

    async fn get_segment_upload_uri(&self, _segment: &VodSegmentId, session_id: &str, _part: i64) -> Result<String, SquadOvError> {
        Ok(session_id.to_string())
    }

    async fn finish_segment_upload(&self, _segment: &VodSegmentId, _session_id: &str, _parts: &[String]) -> Result<(), SquadOvError> {
        Ok(())
    }

    async fn delete_vod(&self, segment: &VodSegmentId) -> Result<(), SquadOvError> {
        let fname = self.segment_id_to_path(segment);
        if !fname.exists() {
            return Err(SquadOvError::NotFound);
        }

        Ok(fs::remove_file(fname)?)
    }

    async fn get_public_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        self.get_segment_redirect_uri(segment).await
    }

    async fn make_segment_public(&self, _segment: &VodSegmentId) -> Result<(), SquadOvError> {
        Ok(())
    }

    async fn check_vod_segment_is_public(&self, _segment: &VodSegmentId) -> Result<bool, SquadOvError> {
        Ok(false)
    }

    async fn get_vod_md5(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        let mut file = fs::File::open(self.segment_id_to_path(segment))?;
        let mut hasher = Md5::default();
        let _n = io::copy(&mut file, &mut hasher);
        Ok(base64::encode(hasher.finalize()))
    }
}