#[macro_use]
extern crate lazy_static;

use lambda_runtime::{handler_fn, Context};
use serde::Deserialize;
use serde_json::{Value};
use squadov_common::{
    SquadOvError,
    encode,
    combatlog::{
        self,
        CombatLogReportIO,
        CombatLogReportGenerator,
        CombatLogReportContainer,
    },
    ff14::reports::Ff14ReportsGenerator,
};
use regex::Regex;
use std::{
    io::{
        Write,
        Seek,
        BufReader,
        BufRead,
    },
    str::FromStr,
    sync::Arc,
    fs::File,
};
use rusoto_core::Region;
use rusoto_s3::{
    S3Client,
    S3,
    ListObjectsV2Request,
    GetObjectRequest,
};
use rusoto_secretsmanager::{
    SecretsManagerClient,
    SecretsManager,
    GetSecretValueRequest,
};
use chrono::{DateTime, Utc};
use tokio::io::AsyncReadExt;
use sqlx::{
    ConnectOptions,
    postgres::{PgPool, PgPoolOptions, PgConnectOptions},
};

#[derive(Deserialize)]
struct Payload {
    #[serde(rename="Records")]
    records: Vec<Record>,
}

#[derive(Deserialize)]
struct Record {
    s3: S3Record,
}

#[derive(Deserialize)]
struct S3Record {
    bucket: S3Bucket,
    object: S3Object,
}

#[derive(Deserialize)]
struct S3Bucket {
    name: String,
}

#[derive(Deserialize)]
struct S3Object {
    key: String,
}

struct SharedClient {
    s3: Arc<S3Client>,
    efs_directory: String,
    pool: Arc<PgPool>,
}

impl SharedClient {
    async fn load_merge_combat_log_data_to_disk(&self, bucket: &str, partition: &str) -> Result<File, SquadOvError> {
        // The data in S3 will be split into multiple compressed files so we want to merge them all into one uncompressed file for us to deal with while processing.
        let parsed_object_prefix = format!("form=Parsed/partition={}/", partition);

        // We need to first get all the files with the given prefix and then we need to sort.
        // We want to sort by date of file creation since Firehose *should* dump all the data out in its
        // current buffer into the same file for each partition.
        let mut continuation_token: Option<String> = None;

        #[derive(Default, Debug)]
        struct S3Key {
            key: String,
            last_modified: Option<DateTime<Utc>>,
        }

        let mut available_keys: Vec<S3Key> = vec![];
        loop {
            let req = ListObjectsV2Request{
                bucket: String::from(bucket),
                continuation_token: continuation_token.clone(),
                prefix: Some(parsed_object_prefix.clone()),
                delimiter: Some(String::from("/")),
                ..ListObjectsV2Request::default()
            };

            let resp = self.s3.list_objects_v2(req).await?;

            if let Some(objects) = resp.contents {
                for obj in objects {
                    let mut obj_key = S3Key::default();

                    if let Some(key) = obj.key {
                        obj_key.key = key;
                    } else {
                        log::warn!("Skipping over object because it doesn't have a key: {:?}", &obj);
                        continue;
                    }

                    obj_key.last_modified = Some(obj.last_modified
                        .map(|x| {
                            DateTime::parse_from_rfc3339(&x)
                        })
                        .map_or(Ok(None), |r| r.map(Some))?
                        .map(|x| {
                            DateTime::from(x)
                        })
                        .unwrap_or(Utc::now()));
                    available_keys.push(obj_key);
                }
            }

            if let Some(is_trunc) = resp.is_truncated {
                if is_trunc {
                    continuation_token = resp.next_continuation_token;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Sort in order of ascending last modified.
        available_keys.sort_by(|a, b| {
            let a_mod = a.last_modified.as_ref().unwrap();
            let b_mod = b.last_modified.as_ref().unwrap();
            a_mod.partial_cmp(b_mod).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Download files one by one, uncompress, and merge them into the temp file.
        let file = tempfile::tempfile_in(&self.efs_directory)?;
        for key in available_keys {
            let req = GetObjectRequest{
                bucket: String::from(bucket),
                key: key.key.clone(),
                ..GetObjectRequest::default()
            };

            let resp = self.s3.get_object(req).await?;

            let mut compressed_data: Vec<u8> = Vec::new();
            if let Some(body) = resp.body {
                let mut body = body.into_async_read();
                body.read(&mut compressed_data).await?;
            } else {
                log::error!("Invalid body for object: {}/{:?}", bucket, &key);
                continue;
            }

            let mut decoder = flate2::write::GzDecoder::new(&file);
            decoder.write_all(&mut compressed_data)?;

            // Note that we do not need to add an extra new line at the end because the parsed data
            // should already contain that new line. Note that our report generation will need to be resilient
            // to new lines.
        }

        Ok(file)
    }

    async fn create_report_generator<'a>(&self, game: &'a str, id: &'a str, work_dir: &'a str) -> Result<Box<dyn CombatLogReportGenerator>, SquadOvError> {
        let report = sqlx::query!(
            "
            SELECT *
            FROM squadov.combat_logs
            WHERE partition_id = $1
            ",
            &format!("{}_{}", game, id)
        )
            .fetch_one(&*self.pool)
            .await?;

        let mut gen = match game {
            "ff14" => CombatLogReportContainer::new(
                // TODO: Pull actual start time from database
                Ff14ReportsGenerator::new(report.start_time)?
            ),
            _ => {
                log::error!("Unsupported game for combat log generation: {}", &game);
                return Err(SquadOvError::BadRequest);
            },
        };
        
        gen.initialize_work_dir(work_dir)?;
        Ok(Box::new(gen))
    }

    async fn generate_reports<'a>(&self, mut gen: Box<dyn CombatLogReportGenerator>, mut file: File) -> Result<Box<dyn CombatLogReportGenerator>, SquadOvError> {
        // Seek to beginning of file just because we were previously writing to the file so the stream offset is probably at the end.
        file.seek(std::io::SeekFrom::Start(0))?;
        let reader = BufReader::new(file);

        for ln in reader.lines() {
            if let Ok(ln) = ln {
                let data = ln.trim();
                if data.is_empty() {
                    continue;
                }
                gen.handle(data)?;
            }
        }

        gen.finalize()?;
        Ok(gen)
    }

    async fn handle_s3_data(&self, data: S3Record) -> Result<(), SquadOvError> {
        // The key is in the form:
        // form=Flush/partition=KEY
        // So we need to parse out the partition key to get 1) the game and 2) the unique ID since it's in the form: GAME_ID.
        // We can dispatch the report generation task based off of the game that we parsed out.
        lazy_static! {
            static ref RE_KEY: Regex = Regex::new(r"form=(?P<form>.*)\/partition=(?P<partition>.*)\/").unwrap();
            static ref RE_MATCH: Regex = Regex::new(r"(?P<game>.*)_(?P<id>.*)").unwrap();
        }

        if let Some(key_cap) = RE_KEY.captures(&data.object.key) {
            let form = key_cap.name("form").map(|x| { String::from(x.as_str()) }).unwrap_or(String::new());
            let partition = encode::url_decode(&key_cap.name("partition").map(|x| { String::from(x.as_str()) }).unwrap_or(String::new()))?;

            // Sanity check the form field first - need to make sure it's "Flush" since that's the only thing
            // we care about.
            if form != "Flush" {
                log::error!("Incorrect form for Combat log report generation: {}", &data.object.key);
                return Err(SquadOvError::BadRequest);
            }

            if let Some(match_cap) = RE_MATCH.captures(&partition) {
                let game = match_cap.name("game").map(|x| { String::from(x.as_str()) }).unwrap_or(String::new());
                let id = match_cap.name("id").map(|x| { String::from(x.as_str()) }).unwrap_or(String::new());
                let mut gen = self.generate_reports(
                    self.create_report_generator(&game, &id, &self.efs_directory).await?,
                    self.load_merge_combat_log_data_to_disk(&data.bucket.name, &partition).await?
                ).await?;
                combatlog::store_static_combat_log_reports(gen.get_reports()?, &data.bucket.name, &partition, self.s3.clone()).await?;
                Ok(())
            } else {
                log::error!("Invalid game partition ID format: {}", &data.object.key);
                Err(SquadOvError::BadRequest)
            }
        } else {
            log::error!("Combat Log S3 Key in the incorrect format: {}", &data.object.key);
            Err(SquadOvError::BadRequest)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_LOG", "info,squadov_common=info,combat_log_report_generator=info");
    env_logger::init();

    let aws_region = std::env::var("SQUADOV_AWS_REGION").unwrap();
    let efs_directory = std::env::var("SQUADOV_EFS_DIRECTORY").unwrap();
    let secret_id = std::env::var("SQUADOV_LAMBDA_DB_SECRET").unwrap();
    let db_host = std::env::var("SQUADOV_LAMBDA_DBHOST").unwrap();
    log::info!("AWS Region: {}", &aws_region);
    log::info!("EFS Directory: {}", &efs_directory);
    log::info!("Secret ID: {}", &secret_id);
    log::info!("DB Host: {}", &db_host);

    log::info!("Creating Secret Manager...");
    let secrets_client = SecretsManagerClient::new(
        Region::from_str(&aws_region)?
    );

    log::info!("Getting DB Secret...");
    let secret_object = secrets_client.get_secret_value(GetSecretValueRequest{
        secret_id,
        ..GetSecretValueRequest::default()
    }).await?;

    // Secret string contains a JSON structure of the form:
    // (it technically has more fields but these are the ones we care about)
    #[derive(Deserialize)]
    struct DbSecret {
        username: String,
        password: String,
    }

    let creds = if let Some(secret_string) = secret_object.secret_string {
        log::info!("...Found Creds.");
        serde_json::from_str::<DbSecret>(&secret_string)?
    } else {
        return Err(SquadOvError::BadRequest);
    };

    let mut conn = PgConnectOptions::new()
        .host(&db_host)
        .username(&creds.username)
        .password(&creds.password)
        .port(5432)
        .application_name("combat_log_report_generator")
        .database("squadov")
        .statement_cache_capacity(0);
    conn.log_statements(log::LevelFilter::Trace);

    log::info!("Creating Shared Client...");
    let shared = SharedClient{
        s3: Arc::new(S3Client::new(
            Region::from_str(&aws_region)?
        )),
        efs_directory,
        pool: Arc::new(PgPoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .max_lifetime(std::time::Duration::from_secs(6*60*60))
            .idle_timeout(std::time::Duration::from_secs(3*60*60))
            .connect_with(conn)
            .await?
        ),
    };

    let shared_ref = &shared;
    let closure = move |event: Value, _ctx: Context| async move {
        log::info!("Handling S3 Event Notification: {:?}", event);

        let payload = serde_json::from_value::<Payload>(event)?;
        for record in payload.records {
            shared_ref.handle_s3_data(record.s3).await?;
        }

        Ok::<(), SquadOvError>(())
    };

    log::info!("Starting Runtime [Combat Log Report Generator]...");
    lambda_runtime::run(handler_fn(closure)).await?;
    Ok(())
}