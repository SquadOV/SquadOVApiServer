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
use futures_util::TryStreamExt;
use rusoto_s3::{
    S3,
    StreamingBody,
    GetObjectRequest,
    GetObjectAclRequest,
    PutObjectAclRequest,
    PutObjectTaggingRequest,
    PutObjectRequest,
    DeleteObjectRequest,
    CreateMultipartUploadRequest,
    UploadPartRequest,
    CompleteMultipartUploadRequest, CompletedMultipartUpload, CompletedPart,
    Tagging,
    Tag,
    util::{
        PreSignedRequest,
        PreSignedRequestOption,
    }
};
use tokio::fs::{
    File as TFile
};
use tokio_util::codec::{BytesCodec, FramedRead};
use rusoto_credential::ProvideAwsCredentials;
use md5::Digest;
use chrono::{Utc, Duration};
use rsa::{
    RsaPrivateKey,
    pkcs1::FromRsaPrivateKey,
    padding::PaddingScheme,
    hash::Hash
};

use async_trait::async_trait;
use actix_web::web::{BytesMut};

const S3_URI_PREFIX : &'static str = "s3://";
const S3_ALL_USERS_GROUP: &'static str = "http://acs.amazonaws.com/groups/global/AllUsers";

pub struct S3VodManager {
    bucket: String,
    aws: Arc<Option<AWSClient>>,
    cdn_private_key: RsaPrivateKey,
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
            cdn_private_key: RsaPrivateKey::read_pkcs1_pem_file(std::path::Path::new(&cdn.private_key_fname))?,
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

                let expires = Utc::now() + Duration::seconds(43200);
                let signature = {
                    let policy = format!(
                        r#"{{"Statement":[{{"Resource":"{base}","Condition":{{"DateLessThan":{{"AWS:EpochTime":{expires}}}}}}}]}}"#,
                        base=&base_url,
                        expires=expires.timestamp(),
                    );

                    // Steps are from copying AWS's reference code:
                    // https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/CreateSignatureInCSharp.html
                    // 1) Create a SHA-1 hash of the actual policy string.
                    // 2) Compute an RSA PKCS1v15 Signature (with SHA-1 hasing) using our private key.
                    // 3) Encode in URL-safe Base 64
                    let policy_hash = {
                        let mut hasher = sha1::Sha1::new();
                        hasher.update(policy.as_bytes());
                        hasher.finalize()
                    };
                    let policy_signature = self.cdn_private_key.sign(PaddingScheme::PKCS1v15Sign{
                        hash: Some(Hash::SHA1)
                    }, &policy_hash)?;
                    base64::encode_config(&policy_signature, base64::URL_SAFE)
                };
                let key_pair_id = self.cdn.public_key_id.clone();

                format!(
                    "{base}?Expires={expires}&Signature={signature}&Key-Pair-Id={keypair}",
                    base=base_url,
                    expires=expires.timestamp(),
                    signature=signature,
                    keypair=key_pair_id
                )
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
        for _i in 0..5 {
            let md5_hash = {
                let mut file = std::fs::File::open(path)?;
                let mut hasher = md5::Md5::new();
                std::io::copy(&mut file, &mut hasher)?;
                let hash = hasher.finalize();
                base64::encode(hash)
            };
    
            let file_byte_size = {
                let file = std::fs::File::open(path)?;
                file.metadata()?.len()
            };

            let file = TFile::open(path).await?;
            let framed_read = FramedRead::new(file, BytesCodec::new()).map_ok(BytesMut::freeze);
            let req = PutObjectRequest{
                bucket: self.bucket.clone(),
                body: Some(
                    StreamingBody::new(framed_read)
                ),
                content_md5: Some(md5_hash),
                content_length: Some(file_byte_size as i64),
                content_type: Some(String::from("application/octet-stream")),
                key: segment.get_fname(),
                ..PutObjectRequest::default()
            };

            match (*self.aws).as_ref().unwrap().s3.put_object(req).await {
                Ok(_) => return Ok(()),
                Err(err) => {
                    log::warn!("Failed to do AWS S3 PUT {:?} - RETRYING", err);
                    continue;
                }
            };
        }
        
        Err(SquadOvError::InternalError(String::from("Failed to Upload VOD - Exceeded retry limit")))
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
                "https://{bucket}.s3.{region}.amazonaws.com/{key}",
                bucket=&self.bucket,
                region=self.client().region.name(),
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
}