use squadov_common::{
    SquadOvError,
    rabbitmq::{
        RabbitMqInterface,
        RabbitMqConfig,
        RabbitMqListener,
    },
};
use structopt::StructOpt;
use std::sync::Arc;
use sqlx::{
    postgres::{
        PgPoolOptions,
        types::PgRange,
        PgPool,
    },
    Row,
};
use uuid::Uuid;
use chrono::{DateTime,Utc};
use std::ops::{Bound, Bound::{
    Excluded,
    Included
}};
use async_trait::async_trait;
use std::collections::HashMap;

pub struct WowTaskHandler {
    src_db: Arc<PgPool>,
    dst_db: Arc<PgPool>,
    encounter: bool,
    characters: bool,
}

impl WowTaskHandler {
    pub fn new (src_db: Arc<PgPool>, dst_db: Arc<PgPool>, encounter: bool, characters: bool) -> Self {
        Self {
            src_db,
            dst_db,
            encounter,
            characters,
        }
    }
}

impl WowTaskHandler {
    async fn handle_character_tasks(&self, task_ids: &[i64]) -> Result<(), SquadOvError> {
        log::info!("\tselect {}", task_ids.len());

        // only want to transfer over the bare minimum to get tasks to show up so the only table data
        // we want to transfer are in:
        // - wow_match_view_character_presence
        // - wow_match_view_combatants
        // - wow_match_view_events
        //
        // And note that we only transfer the bare minimum needed to get users' matches to show up
        // hence only the characters actually in 'wow_match_view_combatants' AND 'wow_match_view_character_presence'
        // should get transferred.
        let tasks = sqlx::query(
            r#"
            SELECT
                wcp.character_id,
                wcp.view_id,
                wcp.unit_guid,
                wcp.unit_name,
                wcp.owner_guid,
                wcp.flags,
                wcp.has_combatant_info,
                mvc.team,
                mvc.spec_id,
                mvc.rating,
                wve.event_id,
                wve.view_id AS "alt_view_id",
                wve.log_line,
                wve.tm
            FROM squadov.wow_match_view_character_presence AS wcp
            INNER JOIN squadov.wow_match_view_combatants AS mvc
                ON mvc.character_id = wcp.character_id
            INNER JOIN squadov.wow_match_view_events AS wve
                ON wve.event_id = mvc.event_id
            WHERE wcp.character_id = ANY($1)
            "#,
        )
            .bind(task_ids)
            .fetch_all(&*self.src_db)
            .await.unwrap();
        log::info!("\tpost select.");

        if tasks.is_empty() {
            return Ok(());
        }
        
        let mut tx = self.dst_db.begin().await.unwrap();

        // Insert into wow_match_view_character_presence and use (view id, unit guid) to get the new character id to use.
        // Results in: old character id -> new character id
        let character_id_mapping: HashMap<i64, i64> = {
            let mut task_sql: Vec<String> = Vec::new();
            task_sql.push(
                String::from("
                    INSERT INTO squadov.wow_match_view_character_presence (
                        view_id,
                        unit_guid,
                        unit_name,
                        owner_guid,
                        flags,
                        has_combatant_info
                    )
                    VALUES
                ")
            );

            for t in &tasks {
                task_sql.push(format!("(
                    {view_id},
                    {unit_guid},
                    {unit_name},
                    {owner_guid},
                    {flags},
                    {has_combatant_info}
                )",
                    view_id=squadov_common::sql_format_string(&t.get::<Uuid, &str>("view_id").to_string()),
                    unit_guid=squadov_common::sql_format_string(&t.get::<String, &str>("unit_guid")),
                    unit_name=squadov_common::sql_format_option_string(&t.get::<Option<String>, &str>("unit_name")),
                    owner_guid=squadov_common::sql_format_option_string(&t.get::<Option<String>, &str>("owner_guid")),
                    flags=t.get::<i64, &str>("flags"),
                    has_combatant_info=t.get::<bool, &str>("has_combatant_info"),
                ));
                task_sql.push(String::from(","));
            }

            task_sql.truncate(task_sql.len() - 1);
            task_sql.push(String::from(" ON CONFLICT DO NOTHING RETURNING character_id, view_id, unit_guid"));
            let result: HashMap<(Uuid, String), i64> = sqlx::query(&task_sql.join("")).fetch_all(&mut tx).await?.into_iter().map(|x| {
                let view_id = x.get::<Uuid, &str>("view_id");
                let unit_guid = x.get::<String, &str>("unit_guid");
                let char_id = x.get::<i64, &str>("character_id");
                ((view_id, unit_guid), char_id)
            }).collect();

            tasks.iter()
                .filter(|x| {
                    let view_id = x.get::<Uuid, &str>("view_id");
                    let unit_guid = x.get::<String, &str>("unit_guid");
                    result.contains_key(&(view_id, unit_guid))
                })
                .map(|x| {
                    let view_id = x.get::<Uuid, &str>("view_id");
                    let unit_guid = x.get::<String, &str>("unit_guid");
                    let new_id = result.get(&(view_id, unit_guid)).unwrap();
                    (x.get::<i64, &str>("character_id"), *new_id)
                }).collect()
        };
        log::info!("\tpost insert characters.");

        // Insert new events first (wow_match_view_events). Use (view id, log line) to get the new event id to use.
        let event_id_mapping: HashMap<i64, i64> = {
            let mut task_sql: Vec<String> = Vec::new();
            task_sql.push(
                String::from("
                    INSERT INTO squadov.wow_match_view_events (
                        view_id,
                        log_line,
                        tm
                    )
                    VALUES
                ")
            );

            for t in &tasks {
                task_sql.push(format!("(
                    {view_id},
                    {log_line},
                    {tm}
                )",
                    view_id=t.get::<i64, &str>("alt_view_id"),
                    log_line=t.get::<i64, &str>("log_line"),
                    tm=squadov_common::sql_format_time(&t.get::<DateTime<Utc>, &str>("tm")),
                ));
                task_sql.push(String::from(","));
            }

            task_sql.truncate(task_sql.len() - 1);
            task_sql.push(String::from(" ON CONFLICT DO NOTHING RETURNING event_id, view_id, log_line"));

            let result: HashMap<(i64, i64), i64> = sqlx::query(&task_sql.join("")).fetch_all(&mut tx).await?.into_iter().map(|x| {
                let view_id = x.get::<i64, &str>("view_id");
                let log_line = x.get::<i64, &str>("log_line");
                let event_id = x.get::<i64, &str>("event_id");
                ((view_id, log_line), event_id)
            }).collect();

            tasks.iter()
                .filter(|x| {
                    let view_id = x.get::<i64, &str>("alt_view_id");
                    let log_line = x.get::<i64, &str>("log_line");
                    result.contains_key(&(view_id, log_line))
                })
                .map(|x| {
                    let view_id = x.get::<i64, &str>("alt_view_id");
                    let log_line = x.get::<i64, &str>("log_line");
                    let new_id = result.get(&(view_id, log_line)).unwrap();
                    (x.get::<i64, &str>("event_id"), *new_id)
                }).collect()
        };
        log::info!("\tpost insert events.");

        // Now use the previous two mappings to insert a new entry into wow_match_view_combatants.
        {
            let mut task_sql: Vec<String> = Vec::new();
            task_sql.push(
                String::from("
                    INSERT INTO squadov.wow_match_view_combatants (
                        event_id,
                        character_id,
                        team,
                        spec_id,
                        rating
                    )
                    VALUES
                ")
            );

            let mut added: bool = false;
            for t in &tasks {
                if let Some(event_id) = event_id_mapping.get(&t.get::<i64, &str>("event_id")) {
                    if let Some(character_id) = character_id_mapping.get(&t.get::<i64, &str>("character_id")) {
                        task_sql.push(format!("(
                            {event_id},
                            {character_id},
                            {team},
                            {spec_id},
                            {rating}
                        )",
                            event_id=event_id,
                            character_id=character_id,
                            team=t.get::<i32, &str>("team"),
                            spec_id=t.get::<i32, &str>("spec_id"),
                            rating=t.get::<i32, &str>("rating"),
                        ));
                        task_sql.push(String::from(","));
                        added = true;
                    }
                }
            }

            if added {
                task_sql.truncate(task_sql.len() - 1);
                task_sql.push(String::from(" ON CONFLICT DO NOTHING"));
                sqlx::query(&task_sql.join("")).execute(&mut tx).await?;
            }
        }
        log::info!("\tpost insert combatants.");
        

        tx.commit().await.unwrap();
        log::info!("\tpost commit.");
        Ok(())
    }

    async fn handle_match_tasks(&self, task_ids: &[Uuid]) -> Result<(), SquadOvError> {
        log::info!("\tselect {}", task_ids.len());
        let tasks = sqlx::query(&format!(
            "
            SELECT *
            FROM squadov.{}
            WHERE match_uuid = ANY($1)
            ",
            if self.encounter {
                "new_wow_encounters"
            } else {
                "new_wow_arenas"
            }))
            .bind(task_ids)
            .fetch_all(&*self.src_db)
            .await.unwrap();
        log::info!("\tpost select.");

        if tasks.is_empty() {
            return Ok(());
        }

        let mut tx = self.dst_db.begin().await.unwrap();

        let mut task_sql: Vec<String> = Vec::new();
        task_sql.push(
            if self.encounter {
                String::from("
                    INSERT INTO squadov.new_wow_encounters (
                        match_uuid,
                        tr,
                        combatants_key,
                        encounter_id,
                        difficulty,
                        instance_id
                    )
                    VALUES
                ")
            } else {
                String::from("
                    INSERT INTO squadov.new_wow_arenas (
                        match_uuid,
                        tr,
                        combatants_key,
                        instance_id,
                        arena_type
                    )
                    VALUES 
                ")
            }
        );

        for t in &tasks {
            let rng = t.get::<PgRange<DateTime<Utc>>, &str>("tr");
            if self.encounter {
                task_sql.push(format!("(
                    {match_uuid},
                    tstzrange({start}::TIMESTAMPTZ, {end}::TIMESTAMPTZ, '[]'),
                    {combatants_key},
                    {encounter_id},
                    {difficulty},
                    {instance_id}
                )",
                    match_uuid=squadov_common::sql_format_string(&t.get::<Uuid, &str>("match_uuid").to_string()),
                    start=squadov_common::sql_format_time(&get_bound(&rng.start)),
                    end=squadov_common::sql_format_time(&get_bound(&rng.end)),
                    combatants_key=squadov_common::sql_format_string(&t.get::<String, &str>("combatants_key")),
                    encounter_id=t.get::<i32, &str>("encounter_id"),
                    difficulty=t.get::<i32, &str>("difficulty"),
                    instance_id=t.get::<i32, &str>("instance_id"),
                ));
            } else {
                task_sql.push(format!("(
                    {match_uuid},
                    tstzrange({start}::TIMESTAMPTZ, {end}::TIMESTAMPTZ, '[]'),
                    {combatants_key},
                    {instance_id},
                    {arena_type}
                )",
                    match_uuid=squadov_common::sql_format_string(&t.get::<Uuid, &str>("match_uuid").to_string()),
                    start=squadov_common::sql_format_time(&get_bound(&rng.start)),
                    end=squadov_common::sql_format_time(&get_bound(&rng.end)),
                    combatants_key=squadov_common::sql_format_string(&t.get::<String, &str>("combatants_key")),
                    instance_id=t.get::<i32, &str>("instance_id"),
                    arena_type=squadov_common::sql_format_string(&t.get::<String, &str>("arena_type")),
                ));
            }
            task_sql.push(String::from(","));
        }

        task_sql.truncate(task_sql.len() - 1);
        task_sql.push(String::from(" ON CONFLICT DO NOTHING"));
        sqlx::query(&task_sql.join("")).execute(&mut tx).await?;
        tx.commit().await.unwrap();
        log::info!("\tpost insert.");
        Ok(())
    }
}

#[async_trait]
impl RabbitMqListener for WowTaskHandler {
    async fn handle(&self, data: &[u8], _queue: &str) -> Result<(), SquadOvError> {
        log::info!("Handle Transfer RabbitMQ Task: {}", if self.characters { "Characters" } else if self.encounter { "Encounter" } else { "Arena" });
        if self.characters {
            let task_ids: Vec<String> = serde_json::from_slice(data)?;
            self.handle_character_tasks(&task_ids.into_iter().map(|x| {
                x.parse::<i64>().unwrap()
            }).collect::<Vec<i64>>()).await
        } else {
            let task_ids: Vec<Uuid> = serde_json::from_slice(data)?;
            self.handle_match_tasks(&task_ids).await
        }
    }
}

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    src: String,
    #[structopt(short, long)]
    dest: String,
    #[structopt(short, long)]
    threads: u32,
    #[structopt(short, long)]
    queue: String,
    #[structopt(short, long)]
    encounter: bool,
    #[structopt(short, long)]
    characters: bool,
    #[structopt(short, long)]
    rmq: String,
}

fn get_bound(bnd: &Bound<DateTime<Utc>>) -> DateTime<Utc> {
    if let Excluded(x) = bnd {
        x.clone()
    } else if let Included(x) = bnd {
        x.clone()
    } else {
        Utc::now()
    }
}

#[tokio::main]
async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,wow_match_transfer=debug");
    env_logger::init();

    let opts = Options::from_args();

    let src_pool = Arc::new(PgPoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .max_lifetime(std::time::Duration::from_secs(6*60*60))
        .idle_timeout(std::time::Duration::from_secs(3*60*60))
        .connect(&opts.src)
        .await
        .unwrap());

    let dst_pool = Arc::new(PgPoolOptions::new()
        .min_connections(1)
        .max_connections(1)
        .max_lifetime(std::time::Duration::from_secs(6*60*60))
        .idle_timeout(std::time::Duration::from_secs(3*60*60))
        .connect(&opts.dest)
        .await
        .unwrap());

    let rabbitmq = RabbitMqInterface::new(&RabbitMqConfig{
        amqp_url: opts.rmq.clone(),
        prefetch_count: 1,
        enable_rso: false,
        rso_queue: String::new(),
        enable_valorant: false,
        valorant_queue: String::new(),
        enable_lol: false,
        lol_queue: String::new(),
        enable_tft: false,
        tft_queue: String::new(),
        enable_vod: false,
        vod_queue: String::new(),
        enable_csgo: false,
        csgo_queue: String::new(),
        enable_steam: false,
        steam_queue: String::new(),
        enable_twitch: false,
        twitch_queue: String::new(),
        additional_queues: Some(vec![
            opts.queue.clone(),
        ])
    }, Some(dst_pool.clone()), true).await.unwrap();

    let handler_itf = Arc::new(WowTaskHandler::new(src_pool.clone(), dst_pool.clone(), opts.encounter, opts.characters));

    for _i in 0..opts.threads {
        RabbitMqInterface::add_listener(rabbitmq.clone(), opts.queue.clone(), handler_itf.clone(), 1).await.unwrap();
    }

    loop {
        async_std::task::sleep(std::time::Duration::from_secs(10)).await;
    }
    Ok(())
}