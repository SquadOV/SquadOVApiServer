use crate::{
    SquadOvError,
    VodSegmentId,
    vod::manager::VodManager,
    aws::{
        AWSClient,
        AWSCDNConfig,
    },
};
use std::sync::Arc;
use rusoto_s3::{
    S3,
    GetObjectRequest,
    GetObjectAclRequest,
    GetObjectTaggingRequest,
    PutObjectAclRequest,
    PutObjectTaggingRequest,
    DeleteObjectRequest,
    CreateMultipartUploadRequest,
    UploadPartRequest,
    CompleteMultipartUploadRequest, CompletedMultipartUpload, CompletedPart,
    HeadObjectRequest,
    Tagging,
    Tag,
    util::{
        PreSignedRequest,
        PreSignedRequestOption,
    }
};
use tokio::{
    fs::{
        File as TFile
    },
    io::AsyncReadExt,
};
use rusoto_credential::ProvideAwsCredentials;
use md5::Digest;
use async_trait::async_trait;

const S3_URI_PREFIX : &'static str = "s3://";
const S3_ALL_USERS_GROUP: &'static str = "http://acs.amazonaws.com/groups/global/AllUsers";

pub struct S3VodManager {
    bucket: String,
    aws: Arc<Option<AWSClient>>,
    cdn: AWSCDNConfig,
}

impl S3VodManager {
    pub async fn new(full_bucket: &str, client: Arc<Option<AWSClient>>, cdn: AWSCDNConfig) -> Result<S3VodManager, SquadOvError> {
        if client.is_none() {
            return Err(SquadOvError::InternalError(String::from("AWS Client not found.")));
        }

        let bucket = full_bucket[S3_URI_PREFIX.len()..].to_string();

        Ok(S3VodManager{
            bucket: bucket.clone(),
            aws: client,
            cdn,
        })
    }

    fn client(&self) -> &AWSClient {
        (*self.aws).as_ref().unwrap()
    }
}

#[async_trait]
impl VodManager for S3VodManager {
    fn manager_type(&self) -> super::VodManagerType {
        super::VodManagerType::S3
    }

    async fn get_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        Ok(
            if !self.cdn.private_cdn_domain.is_empty() {
                // We need to manually sign the CloudFront URL here using the trusted private key
                let base_url = format!(
                    "{base}/{fname}",
                    base=&self.cdn.private_cdn_domain,
                    fname=segment.get_fname(),
                );

                (*self.aws).as_ref().unwrap().sign_cloudfront_url(&base_url)?
            } else {
                let req = GetObjectRequest{
                    bucket: self.bucket.clone(),
                    key: segment.get_fname(),
                    ..GetObjectRequest::default()
                };

                let creds = self.client().provider.credentials().await?;
                let region = self.client().region.clone();

                req.get_presigned_url(&region, &creds, &PreSignedRequestOption{
                    expires_in: std::time::Duration::from_secs(43200)
                })
            }
        )
    }

    async fn download_vod_to_path(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        let req = GetObjectRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            ..GetObjectRequest::default()
        };

        let mut output_file = TFile::create(path).await?;
        let resp = (*self.aws).as_ref().unwrap().s3.get_object(req).await?;
        if let Some(body) = resp.body {
            let mut reader = body.into_async_read();

            // Stream the download from GCS onto disk so we never have to have to entire file in memory.
            tokio::io::copy(&mut reader, &mut output_file).await?;

            Ok(())
        } else {
            Err(SquadOvError::InternalError(String::from("No VOD downloaded from AWS.")))
        }
    }

    async fn upload_vod_from_file(&self, segment: &VodSegmentId, path: &std::path::Path) -> Result<(), SquadOvError> {
        // We need to do a multi-part upload to S3 because otherwise we run the risk of
        //  1) the video being too large so the regular PUT request fails or
        //  2) the time it takes to upload is too long which results in a timeout.
        let upload_id = self.start_segment_upload(segment).await?;

        // Since we're uploading from a file we can pre-determine how many segments we're going to use
        // based off of the file size.
        let mut bytes_left_to_upload = {
            let file = std::fs::File::open(path)?;
            file.metadata()?.len()
        };

        // 100 Megabyte segments should be enough to get some decent
        // upload efficiencies where we aren't constantly uploading small chunks of data.
        let max_part_size_bytes: u64 = 100 * 1024 * 1024;
        let num_parts = (bytes_left_to_upload as f32 / max_part_size_bytes as f32).ceil() as u64;

        let mut file = TFile::open(path).await?;
        let mut parts: Vec<String> = vec![];
        let mut offset: u64 = 0;
        for part in 0..num_parts {
            // We should be able to retry each individual part if a part upload fails for whatever eason
            // to get a reasonable amount of resilience to failure.
            let mut success = false;
            for _i in 0i32..5i32 {
                let part_size_bytes = std::cmp::min(bytes_left_to_upload, max_part_size_bytes);
                let mut buffer: Vec<u8> = vec![0; part_size_bytes as usize];
                file.seek(std::io::SeekFrom::Start(offset)).await?;
                file.read_exact(&mut buffer).await?;

                let md5_hash = {
                    let mut hasher = md5::Md5::new();
                    hasher.update(&buffer);
                    let hash = hasher.finalize();
                    base64::encode(hash)
                };

                let req = UploadPartRequest{
                    bucket: self.bucket.clone(),
                    key: segment.get_fname(),
                    part_number: part as i64 + 1,
                    upload_id: upload_id.clone(),
                    body: Some(
                        buffer.into()
                    ),
                    content_md5: Some(md5_hash),
                    content_length: Some(part_size_bytes as i64),
                    ..UploadPartRequest::default()
                };

                let resp = match (*self.aws).as_ref().unwrap().s3.upload_part(req).await {
                    Ok(r) => r,
                    Err(err) => {
                        log::warn!("Failed to do AWS S3 part upload {:?} - RETRYING", err);
                        async_std::task::sleep(std::time::Duration::from_millis(123)).await;
                        continue;
                    }
                };

                if let Some(e_tag) = resp.e_tag {
                    parts.push(e_tag.clone());
                }

                success = true;
                bytes_left_to_upload -= part_size_bytes;
                offset += part_size_bytes;
                break;
            }

            if !success {
                return Err(SquadOvError::InternalError(String::from("Failed to Upload VOD [multi-part] - Exceeded retry limit for a part")));
            }
        }

        self.finish_segment_upload(segment, &upload_id, &parts).await?;
        Ok(())
    }

    async fn is_vod_session_finished(&self, _session: &str) -> Result<bool, SquadOvError> {
        // No need to check because AWS requires us to finish the upload with a CompleteMultiPartUpload action instead.
        Ok(true)
    }

    async fn start_segment_upload(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        let req = CreateMultipartUploadRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            ..CreateMultipartUploadRequest::default()
        };

        let resp = (*self.aws).as_ref().unwrap().s3.create_multipart_upload(req).await?;
        if let Some(upload_id) = resp.upload_id {
            Ok(upload_id)
        } else {
            Err(SquadOvError::InternalError(String::from("No AWS upload ID returned for multipart upload")))
        }
    }
    
    async fn get_segment_upload_uri(&self, segment: &VodSegmentId, session_id: &str, part: i64) -> Result<String, SquadOvError> {
        let req = UploadPartRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
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

    async fn finish_segment_upload(&self, segment: &VodSegmentId, session_id: &str, parts: &[String]) -> Result<(), SquadOvError> {
        let req = CompleteMultipartUploadRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            multipart_upload: Some(CompletedMultipartUpload{
                parts: Some(parts.iter().enumerate().map(|(idx, x)| {
                    CompletedPart {
                        e_tag: Some(x.clone()),
                        part_number: Some(idx as i64 + 1),
                    }
                }).collect()),
            }),
            upload_id: session_id.to_string(),
            ..CompleteMultipartUploadRequest::default()
        };

        (*self.aws).as_ref().unwrap().s3.complete_multipart_upload(req).await?;
        Ok(())
    }

    async fn delete_vod(&self, segment: &VodSegmentId) -> Result<(), SquadOvError> {
        let req = DeleteObjectRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            ..DeleteObjectRequest::default()
        };

        (*self.aws).as_ref().unwrap().s3.delete_object(req).await?;
        Ok(())
    }

    async fn get_public_segment_redirect_uri(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        Ok(
            format!(
                "{base}/{key}",
                base=&self.cdn.public_cdn_domain,
                key=segment.get_fname(),
            )
        )
    }

    async fn make_segment_public(&self, segment: &VodSegmentId) -> Result<(), SquadOvError> {
        // Need to do both until we can move over to the CDN 100%.
        {
            let req = PutObjectAclRequest{
                bucket: self.bucket.clone(),
                key: segment.get_fname(),
                acl: Some(String::from("public-read")),
                ..PutObjectAclRequest::default()
            };
            self.client().s3.put_object_acl(req).await?;
        }

        {
            let req = PutObjectTaggingRequest{
                bucket: self.bucket.clone(),
                key: segment.get_fname(),
                tagging: Tagging {
                    tag_set: vec![
                        Tag {
                            key: String::from("access"),
                            value: String::from("public"),
                        }
                    ],
                },
                ..PutObjectTaggingRequest::default()
            };

            self.client().s3.put_object_tagging(req).await?;
        }

        
        Ok(())
    }

    async fn check_vod_segment_is_public(&self, segment: &VodSegmentId) -> Result<bool, SquadOvError> {
        // First check the tags and then fall back to the legacy ACL check.
        {
            let req = GetObjectTaggingRequest{
                bucket: self.bucket.clone(),
                key: segment.get_fname(),
                ..GetObjectTaggingRequest::default()
            };
            let tags = self.client().s3.get_object_tagging(req).await?;
            for t in tags.tag_set {
                if t.key == "access" && t.value == "public" {
                    return Ok(true);
                }
            }
        }

        let req = GetObjectAclRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            ..GetObjectAclRequest::default()
        };

        let resp = self.client().s3.get_object_acl(req).await?;
        Ok(
            if let Some(grants) = resp.grants {
                let mut is_public = false;
                for g in grants {
                    // We need the "READ" permission.
                    if let Some(permission) = g.permission {
                        if permission != "READ" {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    // Assigned to the All Users grantee
                    if let Some(grantee) = g.grantee {
                        if let Some(uri) = grantee.uri {
                            if uri == S3_ALL_USERS_GROUP {
                                is_public = true;
                                break;
                            }
                        }
                    }
                }

                is_public
            } else {
                false
            }
        )
    }

    async fn get_vod_md5(&self, segment: &VodSegmentId) -> Result<String, SquadOvError> {
        // Note that this isn't entirely accurate - the AWS S3 object's ETag isn't necessarily
        // the MD5 hash if the file is uploaded using multipart upload. Thus we really do need
        // to verify on the client side that the VOD is fastified first before trying to download.
        let req = HeadObjectRequest{
            bucket: self.bucket.clone(),
            key: segment.get_fname(),
            ..HeadObjectRequest::default()
        };
        let resp = self.client().s3.head_object(req).await?;
        Ok(resp.e_tag.unwrap_or(String::new()).replace("\"", ""))
    }
}