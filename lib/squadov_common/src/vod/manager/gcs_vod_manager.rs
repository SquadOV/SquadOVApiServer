use crate::{
    GCPClient,
    gcp::gcs::GCSUploadStatus,
    SquadOvError,
    VodSegmentId,
    vod::manager::VodManager
};
use std::sync::Arc;
use std::collections::BTreeMap;
use std::io::Write;
use std::fs::File;
use std::io::Read;

use async_trait::async_trait;
use rand::Rng;
use actix_web::web::{BytesMut, Bytes};

const GS_URI_PREFIX : &'static str = "gs://";
const MAX_GCS_RETRIES: i32 = 10;
const MAX_GCS_BACKOFF_TIME_MS: i64 = 32000;

pub struct GCSVodManager {
    bucket: String,
    client: Arc<Option<GCPClient>>
}

impl GCSVodManager {
    pub async fn new(full_bucket: &str, client: Arc<Option<GCPClient>>) -> Result<GCSVodManager, SquadOvError> {
        if client.is_none() {
            return Err(SquadOvError::InternalError(String::from("GCP Client not found.")));
        }

        let bucket = full_bucket[GS_URI_PREFIX.len()..].to_string();

        // Do a sanity check to make sure the bucket exists to protect against dev typos!!!!
        // If this fails we'll force panic and fail
        client.as_ref().as_ref().unwrap().gcs().get_bucket(&bucket).await?;

        Ok(GCSVodManager{
            bucket: bucket.clone(),
            client: client,
        })
    }

    fn get_gcp_client(&self) -> &GCPClient {
        (*self.client).as_ref().unwrap()
    }

    fn get_path_parts_from_segment_id(&self, segment: &VodSegmentId) -> Vec<String> {
        vec![segment.video_uuid.to_string(), segment.quality.clone(), segment.segment_name.clone()]
    }

    fn get_fname_from_segment_id(&self, segment: &VodSegmentId) -> String {
        self.get_path_parts_from_segment_id(segment).join("/")
    }
}

#[async_trait]
impl VodManager for GCSVodManager {
    fn manager_type(&self) -> super::VodManagerType {
        super::VodManagerType::GCS
    }

    async fn get_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {       
        let fname = self.get_fname_from_segment_id(segment);
        let client = self.get_gcp_client().gcs();

        // Do not insert a check using the Google Cloud Storage API on whether or not the object exists.
        // The GET request will lag behind the user actually finishing the uploading of the object - just
        // give them the URL and if the download fails then oh well.
        client.create_signed_url("GET", &format!("/{}/{}", &self.bucket, fname), &BTreeMap::new())
    }

    async fn download_vod_to_path(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        let uri = self.get_segment_redirect_uri(segment).await?;
        let resp = reqwest::get(&uri).await?;
        let mut output_file = std::fs::File::create(path)?;
        let body = resp.bytes().await?;
        output_file.write_all(&body)?;
        Ok(())
    }

    async fn upload_vod_from_file(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        let client = self.get_gcp_client().gcs();
        let fname = self.get_path_parts_from_segment_id(segment);
        
        // Stream from the file to GCS.
        let mut f = File::open(path)?;
        let mut buffer = BytesMut::with_capacity(8 * 1024 * 1024);
        buffer.resize(8 * 1024 * 1024, 0);

        log::info!("Start resumable upload session: {:?}", segment);
        let session = client.initiate_resumable_upload_session(&self.bucket, &fname).await?;
        log::info!("Obtained resumable upload session: {:?} :: {}", segment, session);
        let mut total_bytes: usize = 0;
        loop {
            let n = f.read(&mut buffer)?;
            let last = n < buffer.len();
            let mut success = false;
            for i in 0..MAX_GCS_RETRIES {
                let byte_buffer = Bytes::copy_from_slice(&buffer[0..n]);
                match client.upload_resumable_object(&session, total_bytes, byte_buffer, last).await {
                    Ok(_) => {
                        success = true;
                        break;
                    },
                    Err(err) => {
                        let backoff_ms = {
                            let mut rng = rand::thread_rng();
                            std::cmp::min(2i64.pow(i as u32) +  rng.gen_range(0..1000), MAX_GCS_BACKOFF_TIME_MS)
                        };
                        log::warn!("Failed to upload to GCS: {:?} - Retrying {} @{}ms", err, i, backoff_ms);
                        async_std::task::sleep(std::time::Duration::from_millis(backoff_ms as u64)).await;
                    }
                }
            }

            if !success {
                return Err(SquadOvError::InternalError(String::from("Max GCS upload retry limit exceeded.")));
            }
            
            total_bytes += n;
            if last {
                break;
            }
        }

        Ok(())
    }

    async fn is_vod_session_finished(&self, session: &str) -> Result<bool, SquadOvError> {
        let client = self.get_gcp_client().gcs();
        Ok(client.get_upload_status(session).await? == GCSUploadStatus::Complete)
    }
    
    async fn start_segment_upload(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        let fname = self.get_fname_from_segment_id(segment);
        let client = self.get_gcp_client().gcs();

        // Unlike redirect function, we actually want the get_object to *fail* to ensure
        // that a video of the same name doesn't already exists in GCS.
        match client.get_object(&self.bucket, &fname).await {
            Ok(_) => return Err(SquadOvError::BadRequest),
            Err(err) => match err {
                SquadOvError::NotFound => (),
                _ => return Err(err)
            }
        };

        let mut headers = BTreeMap::new();
        headers.insert("x-goog-resumable".to_string(), vec!["start".to_string()]);
        headers.insert("content-type".to_string(), vec!["application/octet-stream".to_string()]);

        let start_resumable_url = client.create_signed_url("POST", &format!("/{}/{}", &self.bucket, fname), &headers)?;

        // Need to send an HTTP post request (empty json body) to the start resumable URL location.
        // Additional headers needed:
        // x-goog-resumable: start
        // content-type: application/octet-stream
        // Expect a 201 response.
        let client = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(60))
            .build()?;

        let result = client
            .post(&start_resumable_url)
            .header("x-goog-resumable", "start")
            .header("content-type", "application/octet-stream")
            .header("content-length", "0")
            .send()
            .await?;
        
        let status = result.status().as_u16();
        if status != 201 {
            return Err(SquadOvError::InternalError(format!("Failed to start Google Cloud resumable upload [{}]: {}", status, result.text().await?)));
        }

        // The URL that the user should upload to (and what we consider to be the "session_id") is stored
        // in the "Location" header in the response.
        if let Some(loc) = result.headers().get("Location") {
            Ok(loc.to_str()?.to_string())
        } else {
            Err(SquadOvError::InternalError(String::from("No location header detected in Google Cloud resumable upload return.")))
        }
    }

    async fn get_segment_upload_uri(&self, _segment: &VodSegmentId, session_id: &str, _part: i64) -> Result<String, SquadOvError> {
        Ok(session_id.to_string())
    }

    async fn finish_segment_upload(&self, _segment: &VodSegmentId, _session_id: &str, _parts: &[String]) -> Result<(), SquadOvError> {
        Ok(())
    }

    async fn delete_vod(&self, segment: &VodSegmentId) -> Result<(), SquadOvError> {
        let fname = self.get_fname_from_segment_id(segment);
        let client = self.get_gcp_client().gcs();
        Ok(client.delete_object(&self.bucket, &fname).await?)
    }

    async fn get_public_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        let fname = self.get_fname_from_segment_id(segment);
        Ok(
            format!(
                "https://storage.googleapis.com/{}/{}",
                self.bucket,
                fname
            )
        )
    }

    async fn make_segment_public(&self, segment: &VodSegmentId) -> Result<(), SquadOvError> {
        let fname = self.get_fname_from_segment_id(segment);
        let client = self.get_gcp_client().gcs();
        Ok(client.set_object_public_acl(&self.bucket, &fname).await?)
    }

    async fn check_vod_segment_is_public(&self, segment: &VodSegmentId) -> Result<bool, SquadOvError> {
        let fname = self.get_fname_from_segment_id(segment);
        let client = self.get_gcp_client().gcs();
        Ok(client.check_object_public_acl(&self.bucket, &fname).await?)
    }

    async fn get_vod_md5(&self, _segment: &VodSegmentId) -> Result<String, SquadOvError> {
        // TODO: The Google Cloud videos should be sufficiently old at this point that it's unlikely
        // that users will be downloading their old VODs.
        Ok(String::new())
    }
}