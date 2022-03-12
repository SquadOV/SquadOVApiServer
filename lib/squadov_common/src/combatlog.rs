pub mod io;
pub mod agg;

use crate::SquadOvError;
use rusoto_s3::{
    S3Client,
    S3,
    UploadPartRequest,
    CreateMultipartUploadRequest,
    CompleteMultipartUploadRequest,
    CompletedMultipartUpload,
    CompletedPart,
};
use std::sync::Arc;
use tokio::{
    fs::File,
    io::{
        AsyncReadExt,
        AsyncSeekExt,
    },
};
use md5::Digest;
use rand::{
    Rng,
    SeedableRng,
};
use serde::{de::DeserializeOwned};

pub struct RawStaticCombatLogReport {
    // Name of the file we store into S3.
    pub key_name: String,
    // The file that contains the data on disk.
    pub raw_file: File,
    // The 'type' of the report. This number depends on the game we're generating the report for.
    pub canonical_type: i32,
}

pub trait CombatLogReportHandler {
    type Data;

    fn handle(&mut self, data: &Self::Data) -> Result<(), SquadOvError>;
}

pub trait CombatLogReportIO {
    fn finalize(&mut self) -> Result<(), SquadOvError>;
    fn initialize_work_dir(&mut self, dir: &str) -> Result<(), SquadOvError>;
    fn get_reports(&mut self) -> Result<Vec<RawStaticCombatLogReport>, SquadOvError>;
}

pub trait CombatLogReportParser {
    fn handle(&mut self, data: &str) -> Result<(), SquadOvError>;
}

pub trait CombatLogReportGenerator: CombatLogReportParser + CombatLogReportIO {}

pub struct CombatLogReportContainer<T> {
    generator: T,
}

impl<T> CombatLogReportGenerator for CombatLogReportContainer<T>
where
    T: CombatLogReportHandler + CombatLogReportIO,
    T::Data: DeserializeOwned,
{}

impl<T> CombatLogReportParser for CombatLogReportContainer<T>
where
    T: CombatLogReportHandler,
    T::Data: DeserializeOwned,
{
    fn handle(&mut self, data: &str) -> Result<(), SquadOvError> {
        let data = serde_json::from_str::<T::Data>(data)?;
        self.generator.handle(&data)?;
        Ok(())
    }
}

impl<T> CombatLogReportIO for CombatLogReportContainer<T>
where
    T: CombatLogReportIO
{
    fn finalize(&mut self) -> Result<(), SquadOvError> {
        self.generator.finalize()
    }

    fn initialize_work_dir(&mut self, dir: &str) -> Result<(), SquadOvError> {
        self.generator.initialize_work_dir(dir)
    }

    fn get_reports(&mut self) -> Result<Vec<RawStaticCombatLogReport>, SquadOvError> {
        self.generator.get_reports()
    }
}

impl<T> CombatLogReportContainer<T> {
    pub fn new(generator: T) -> Self {
        Self {
            generator,
        }
    }
}

const MULTIPART_SEGMENT_SIZE_BYTES: u64 = 100 * 1024 * 1024;

async fn store_single_static_report(mut report: RawStaticCombatLogReport, bucket: &str, partition: &str, s3: Arc<S3Client>) -> Result<(), SquadOvError> {
    let mut rng = rand::rngs::StdRng::from_entropy();
    let key = format!(
        "form=Report/partition={partition}/canonical={canonical}/{name}",
        partition=partition,
        canonical=report.canonical_type,
        name=&report.key_name,
    );

    let upload_id = {
        let req = CreateMultipartUploadRequest{
            bucket: String::from(bucket),
            key: key.clone(),
            ..CreateMultipartUploadRequest::default()
        };

        s3.create_multipart_upload(req).await?.upload_id.ok_or(SquadOvError::InternalError(String::from("No AWS upload ID returned for multipart upload")))?
    };

    // I imagine that it's unlikely that the report will ever get to the size where multipart upload
    // seriously needs to be considered but let's leave it here just in case to be robust. Note that this
    // code is duplicated from the AWS VOD manager. Ideally we'd consolidate...
    let mut bytes_left_to_upload = report.raw_file.metadata().await?.len();
    let num_parts = (bytes_left_to_upload as f32 / MULTIPART_SEGMENT_SIZE_BYTES as f32).ceil() as u64;

    let mut parts: Vec<String> = vec![];
    let mut offset: u64 = 0;
    for part in 0..num_parts {
        let mut success = false;
        for i in 0u32..5u32 {
            let part_size_bytes = std::cmp::min(bytes_left_to_upload, MULTIPART_SEGMENT_SIZE_BYTES);

            let mut buffer: Vec<u8> = vec![0; part_size_bytes as usize];
            report.raw_file.seek(std::io::SeekFrom::Start(offset)).await?;
            report.raw_file.read_exact(&mut buffer).await?;

            let md5_hash = {
                let mut hasher = md5::Md5::new();
                hasher.update(&buffer);
                let hash = hasher.finalize();
                base64::encode(hash)
            };

            let req = UploadPartRequest{
                bucket: String::from(bucket),
                key: key.clone(),
                part_number: part as i64 + 1,
                upload_id: upload_id.clone(),
                body: Some(
                    buffer.into()
                ),
                content_md5: Some(md5_hash),
                content_length: Some(part_size_bytes as i64),
                ..UploadPartRequest::default()
            };

            let resp = match s3.upload_part(req).await {
                Ok(r) => r,
                Err(err) => {
                    log::warn!("Failed to do AWS S3 part upload {:?} - RETRYING", err);
                    async_std::task::sleep(std::time::Duration::from_millis(100u64 + 2u64.pow(i) + rng.gen_range(0..1000))).await;
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
            return Err(SquadOvError::InternalError(String::from("Failed to Upload Report [multi-part] - Exceeded retry limit for a part")));
        }
    }

    let req = CompleteMultipartUploadRequest{
        bucket: String::from(bucket),
        key: key.clone(),
        multipart_upload: Some(CompletedMultipartUpload{
            parts: Some(parts.iter().enumerate().map(|(idx, x)| {
                CompletedPart {
                    e_tag: Some(x.clone()),
                    part_number: Some(idx as i64 + 1),
                }
            }).collect()),
        }),
        upload_id: upload_id.to_string(),
        ..CompleteMultipartUploadRequest::default()
    };

    s3.complete_multipart_upload(req).await?;
    Ok(())
}

pub async fn store_static_combat_log_reports<'a>(reports: Vec<RawStaticCombatLogReport>, bucket: &'a str, partition: &'a str, s3: Arc<S3Client>) -> Result<(), SquadOvError> {
    let handles = reports.into_iter()
        .map(|report| {
            let bucket = String::from(bucket);
            let partition = String::from(partition);
            let s3 = s3.clone();
            tokio::task::spawn(async move {
                store_single_static_report(report, &bucket, &partition, s3).await?;
                Ok::<(), SquadOvError>(())
            })
        })
        .collect::<Vec<_>>();

    for h in handles {
        h.await??;
    }
    Ok(())
}