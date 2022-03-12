pub mod aws;
pub mod gcp;

use crate::SquadOvError;
use sqlx::{Transaction, Executor, Postgres};
use uuid::Uuid;
use async_trait::async_trait;
use std::io::Read;
use std::sync::Arc;
use sqlx::postgres::{PgPool};

pub enum BlobManagerType {
    GCS,
    S3,
}

pub fn get_blob_manager_type(root: &str) -> BlobManagerType {
    if root.starts_with("gs://") {
        BlobManagerType::GCS
    } else if root.starts_with("s3://") {
        BlobManagerType::S3
    } else {
        panic!("Unknown blob manager type.");
    }
}

#[async_trait]
pub trait BlobStorageClient {
    async fn upload_object(&self, bucket_id: &str, path_parts: &Vec<String>, data: &[u8]) -> Result<(), SquadOvError>;
    async fn download_object(&self, bucket_id: &str, path: &str) -> Result<Vec<u8>, SquadOvError>;
    fn strip_bucket_prefix(&self, bucket: &str) -> String;
    fn get_public_url(&self, bucket: &str, path: &str) -> Result<String, SquadOvError>;
}

pub struct BlobManagementClient {
    full_bucket: String,
    bucket: String,
    storage: Arc<dyn BlobStorageClient + Send + Sync>,
    db: Arc<PgPool>,
}

impl BlobManagementClient {
    pub fn new(bucket: &str, db: Arc<PgPool>, storage: Arc<dyn BlobStorageClient + Send + Sync>) -> Self {
        Self {
            full_bucket: bucket.to_string(),
            bucket: storage.strip_bucket_prefix(bucket),
            storage,
            db,
        }
    }

    pub async fn store_new_blob(&self, tx : &mut Transaction<'_, Postgres>, bytes: &[u8], compress: bool) -> Result<Uuid, SquadOvError> {
        // Let's assume that blobs are large enough for compresssion to make a difference.
        let mut compressed_bytes: Vec<u8> = Vec::new();
        if compress {
            // A quality of 6 seems to be a good balanace between size and speed.
            let mut compressor = brotli::CompressorReader::with_params(
                bytes,
                4096,
                &{
                    let mut params = brotli::enc::BrotliEncoderParams::default();
                    params.quality = 6;
                    params
                },
            );
            compressor.read_to_end(&mut compressed_bytes)?;
        }

        let uuid = Uuid::new_v4();
        let local_path = uuid.to_string();
        sqlx::query!(
            "
            INSERT INTO squadov.blob_link_storage (
                uuid,
                bucket,
                local_path
            )
            VALUES (
                $1,
                $2,
                $3
            )
            ",
            uuid,
            &self.full_bucket,
            &local_path,
        )
            .execute(tx)
            .await?;

        self.storage.upload_object(&self.bucket, &vec![local_path.clone()], if compress { &compressed_bytes } else { bytes }).await?;
        Ok(uuid)
    }

    pub async fn get_blob(&self, blob_uuid: &Uuid, is_compressed: bool) -> Result<Vec<u8>, SquadOvError> {
        let data = sqlx::query!(
            "
            SELECT bucket, local_path
            FROM squadov.blob_link_storage
            WHERE uuid = $1
            ",
            blob_uuid
        )
            .fetch_optional(&*self.db)
            .await?;
        
        if data.is_none() {
            return Err(crate::SquadOvError::NotFound)
        }

        let data = data.unwrap();
        let compressed_bytes = self.storage.download_object(&self.storage.strip_bucket_prefix(&data.bucket), &data.local_path).await?;

        if is_compressed {
            let mut uncompressed_bytes: Vec<u8> = Vec::new();
            {
                let mut decompressor = brotli::Decompressor::new(&compressed_bytes[..], 4096);
                decompressor.read_to_end(&mut uncompressed_bytes)?;
            }
            Ok(uncompressed_bytes)
        } else {
            Ok(compressed_bytes)
        }
    }

    pub async fn store_new_json_blob(&self, tx : &mut Transaction<'_, Postgres>, val: &serde_json::Value) -> Result<Uuid, SquadOvError> {
        self.store_new_blob(tx, &serde_json::to_vec(val)?, true).await
    }

    pub async fn get_blob_url(&self, blob_uuid: &Uuid) -> Result<String, SquadOvError> {
        let data = sqlx::query!(
            "
            SELECT bucket, local_path
            FROM squadov.blob_link_storage
            WHERE uuid = $1
            ",
            blob_uuid
        )
            .fetch_one(&*self.db)
            .await?;
        
        self.storage.get_public_url(&self.storage.strip_bucket_prefix(&data.bucket), &data.local_path)
    }
}

pub async fn get_blob_bucket<'a, T>(ex: T, blob_uuid: &Uuid) -> Result<String, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT bucket
            FROM squadov.blob_link_storage
            WHERE uuid = $1
            ",
            blob_uuid
        )
            .fetch_one(ex)
            .await?
            .bucket
    )
}