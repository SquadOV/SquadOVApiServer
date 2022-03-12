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
        PgPool,
    },
};
use uuid::Uuid;
use async_trait::async_trait;

pub struct WowTaskHandler {
    pool: Arc<PgPool>,
}

impl WowTaskHandler {
    pub fn new (pool: Arc<PgPool>) -> Self {
        Self {
            pool,
        }
    }
}

#[async_trait]
impl RabbitMqListener for WowTaskHandler {
    async fn handle(&self, data: &[u8], _queue: &str) -> Result<(), SquadOvError> {
        let view_ids: Vec<Uuid> = serde_json::from_slice(data)?;
        log::info!("Handle Transfer RabbitMQ Task: {:?}", &view_ids);
        for view_id in view_ids {
            sqlx::query!(
                r#"
                UPDATE squadov.wow_match_view AS wmv
                SET player_rating = sub.player_rating,
                    player_spec = sub.player_spec,
                    player_team = sub.player_team,
                    t0_specs = sub.t0_specs,
                    t1_specs = sub.t1_specs
                FROM (
                    SELECT
                        wmv.id,
                        wvc.spec_id AS "player_spec",
                        wvc.rating AS "player_rating",
                        wvc.team AS "player_team",
                        t0.s AS "t0_specs",
                        t1.s AS "t1_specs"
                    FROM squadov.wow_match_view AS wmv
                    LEFT JOIN LATERAL (
                        SELECT wcp.character_id
                        FROM squadov.wow_match_view_character_presence AS wcp
                        INNER JOIN squadov.wow_user_character_cache AS wucc
                            ON wucc.user_id = wmv.user_id
                                AND wucc.unit_guid = wcp.unit_guid
                        WHERE wcp.view_id = wmv.id
                    ) AS wcp
                        ON TRUE
                    LEFT JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    LEFT JOIN LATERAL (
                        SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                        FROM (
                            SELECT MIN(wvc.spec_id)
                            FROM squadov.wow_match_view_character_presence AS wcp
                            INNER JOIN squadov.wow_match_view_combatants AS wvc
                                ON wvc.character_id = wcp.character_id
                            WHERE wcp.view_id = wmv.id
                                AND wvc.team = 0
                            GROUP BY wcp.view_id, wcp.unit_guid
                        ) sub(val)
                    ) AS t0(s)
                        ON TRUE
                    LEFT JOIN LATERAL (
                        SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                        FROM (
                            SELECT MIN(wvc.spec_id)
                            FROM squadov.wow_match_view_character_presence AS wcp
                            INNER JOIN squadov.wow_match_view_combatants AS wvc
                                ON wvc.character_id = wcp.character_id
                            WHERE wcp.view_id = wmv.id
                                AND wvc.team = 1
                            GROUP BY wcp.view_id, wcp.unit_guid
                        ) sub(val)
                    ) AS t1(s)
                        ON TRUE
                    WHERE wmv.id = $1
                ) AS sub
                WHERE sub.id = wmv.id
                "#,
                &view_id,
            )
                .execute(&*self.pool)
                .await?;
        }

        Ok(())
    }
}

#[derive(StructOpt, Debug)]
struct Options {
    #[structopt(short, long)]
    db: String,
    #[structopt(short, long)]
    queue: String,
    #[structopt(short, long)]
    rmq: String,
    #[structopt(short, long)]
    threads: u32,
}

#[tokio::main]
async fn main() -> Result<(), SquadOvError> {
    std::env::set_var("RUST_BACKTRACE", "1");
    std::env::set_var("RUST_LOG", "info,wow_match_view_data_backfill=debug");
    env_logger::init();

    let opts = Options::from_args();

    let pool = Arc::new(PgPoolOptions::new()
        .min_connections(1)
        .max_connections(2)
        .max_lifetime(std::time::Duration::from_secs(6*60*60))
        .idle_timeout(std::time::Duration::from_secs(3*60*60))
        .connect(&opts.db)
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
    }, Some(pool.clone()), true).await.unwrap();

    let handler_itf = Arc::new(WowTaskHandler::new(pool.clone()));
    for _i in 0..opts.threads {
        RabbitMqInterface::add_listener(rabbitmq.clone(), opts.queue.clone(), handler_itf.clone(), 1).await.unwrap();
    }

    loop {
        async_std::task::sleep(std::time::Duration::from_secs(10)).await;
    }
    Ok(())
}