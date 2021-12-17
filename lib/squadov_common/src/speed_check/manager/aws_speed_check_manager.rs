use crate::{
    SquadOvError,
    speed_check::manager::SpeedCheckManager,
    aws::{
        AWSClient,
    },
};
use uuid::Uuid;
use std::sync::Arc;
use rusoto_s3::{
    S3,
    CreateMultipartUploadRequest,
    UploadPartRequest,
    util::{
        PreSignedRequest,
        PreSignedRequestOption,
    }
};
use rusoto_credential::ProvideAwsCredentials;
use async_trait::async_trait;

const S3_URI_PREFIX : &'static str = "s3://";

pub struct S3SpeedCheckManager {
    bucket: String,
    aws: Arc<Option<AWSClient>>,
}

impl S3SpeedCheckManager {
    pub async fn new(full_bucket: &str, client: Arc<Option<AWSClient>>) -> Result<S3SpeedCheckManager, SquadOvError> {
        if client.is_none() {
            return Err(SquadOvError::InternalError(String::from("AWS Client not found.")));
        }

        let bucket = full_bucket[S3_URI_PREFIX.len()..].to_string();

        Ok(S3SpeedCheckManager{
            bucket: bucket.clone(),
            aws: client,
        })
    }

    fn client(&self) -> &AWSClient {
        (*self.aws).as_ref().unwrap()
    }
}

#[async_trait]
impl SpeedCheckManager for S3SpeedCheckManager {
    fn manager_type(&self) -> super::UploadManagerType {
        super::UploadManagerType::S3
    }

    async fn start_speed_check_upload(&self, file_name_uuid: &Uuid) -> Result<String, SquadOvError> {
        let req = CreateMultipartUploadRequest{
            bucket: self.bucket.clone(),
            key: file_name_uuid.to_string(),
            ..CreateMultipartUploadRequest::default()
        };

        let resp = (*self.aws).as_ref().unwrap().s3.create_multipart_upload(req).await?;
        if let Some(upload_id) = resp.upload_id {
            Ok(upload_id)
        } else {
            Err(SquadOvError::InternalError(String::from("No AWS upload ID returned for multipart upload")))
        }
    }
    
    async fn get_speed_check_upload_uri(&self, file_name_uuid: &Uuid, session_id: &str, part: i64) -> Result<String, SquadOvError> {
        let req = UploadPartRequest{
            bucket: self.bucket.clone(),
            key: file_name_uuid.to_string(),
            part_number: part,
            upload_id: session_id.to_string(),
            ..UploadPartRequest::default()
        };

        let creds = self.client().provider.credentials().await?;
        let region = self.client().region.clone();

        Ok(req.get_presigned_url(&region, &creds, &PreSignedRequestOption{
            expires_in: std::time::Duration::from_secs(43200)
        }))
    }
}