use async_trait::async_trait;
use crate::{
    SquadOvError,
    rabbitmq::{RABBITMQ_DEFAULT_PRIORITY, RabbitMqInterface, RabbitMqListener, RabbitMqConfig},
    share::{
        self,
        MatchVideoShareConnection,
    },
    games,
    SquadOvGames,
    VodAssociation,
};
use sqlx::{
    Transaction,
    Postgres,
    postgres::PgPool,
};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use uuid::Uuid;

pub struct SharingRabbitmqInterface {
    mqconfig: RabbitMqConfig,
    rmq: Arc<RabbitMqInterface>,
    db: Arc<PgPool>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SharingTask {
    ShareToSquad {
        user_id: i64,
        match_uuid: Uuid,
        game: SquadOvGames,
        squad_id: i64,
        conn: MatchVideoShareConnection,
        parent_connection_id: Option<i64>,
    },
}

const MAX_AGE_SECONDS: i64 = 86400; // 1 day

impl SharingRabbitmqInterface {
    pub fn new (mqconfig: RabbitMqConfig, rmq: Arc<RabbitMqInterface>, db: Arc<PgPool>) -> Self {
        Self {
            mqconfig,
            rmq,
            db,
        }
    }

    pub async fn handle_vod_share_to_squad(&self, tx : &mut Transaction<'_, Postgres>, user_id: i64, match_uuid: &Uuid, game: SquadOvGames, squad_id: i64, conn: &MatchVideoShareConnection, parent_connection_id: Option<i64>) -> Result<Option<MatchVideoShareConnection>, SquadOvError> {
        let settings = share::get_squad_sharing_settings(&*self.db, squad_id).await?;
        if settings.disabled_games.contains(&game) {
            return Ok(None);
        }

        if game == SquadOvGames::WorldOfWarcraft {
            // Easiest to do a database check here using the parameters we found in the squad sharing settings rather than pulling in a
            // bunch of information about the different possible types of wow match views and doing the check here on the server.
            let prevent_sharing = sqlx::query!(
                r#"
                SELECT (($3::BOOLEAN AND wev.view_id IS NOT NULL) 
                    OR ($4::BOOLEAN AND (wiv.view_id IS NOT NULL AND wiv.instance_type = 1))
                    OR ($5::BOOLEAN AND wcv.view_id IS NOT NULL)
                    OR ($6::BOOLEAN AND 
                        (
                            wav.view_id IS NOT NULL
                                OR (
                                    wiv.view_id IS NOT NULL AND wiv.instance_type = 4
                                )
                        )
                    )
                    OR ($7::BOOLEAN AND (wiv.view_id IS NOT NULL AND wiv.instance_type = 3))
                    OR (wmv.build_version LIKE ANY ($8::VARCHAR[]))
                ) AS "value!"
                FROM squadov.wow_match_view AS wmv
                LEFT JOIN squadov.wow_encounter_view AS wev
                    ON wev.view_id = wmv.id
                LEFT JOIN squadov.wow_challenge_view AS wcv
                    ON wcv.view_id = wmv.id
                LEFT JOIN squadov.wow_arena_view AS wav
                    ON wav.view_id = wmv.id
                LEFT JOIN squadov.wow_instance_view AS wiv
                    ON wiv.view_id = wmv.id
                WHERE wmv.match_uuid = $1
                    AND wmv.user_id = $2
                "#,
                match_uuid,
                user_id,
                settings.wow.disable_encounters,
                settings.wow.disable_dungeons,
                settings.wow.disable_keystones,
                settings.wow.disable_arenas,
                settings.wow.disable_bgs,
                &settings.wow.disabled_releases.iter().map(|x| {
                    String::from(games::wow_release_to_db_build_expression(*x))
                }).collect::<Vec<String>>(),
            )
                .fetch_one(&mut *tx)
                .await?
                .value;
            
            if prevent_sharing {
                return Ok(None);
            }
        }

        // At this point we also need to check the blacklist. If the user is blacklisted they are not allowed to
        // share VODs with the squad even if they leave and rejoin.
        let is_on_blacklist = sqlx::query!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM squadov.squad_user_share_blacklist
                WHERE squad_id = $1 AND user_id = $2
            ) AS "exists!"
            "#,
            squad_id,
            user_id,
        )
            .fetch_one(&mut *tx)
            .await?
            .exists;

        if is_on_blacklist {
            return Ok(None);
        }

        Ok(Some(share::create_new_share_connection(&mut *tx, conn, user_id, parent_connection_id).await?))
    }

    pub async fn request_vod_share_to_squad(&self, user_id: i64, match_uuid: &Uuid, game: SquadOvGames, squad_id: i64, conn: &MatchVideoShareConnection, parent_connection_id: Option<i64>) -> Result<(), SquadOvError> {
        self.rmq.publish(&self.mqconfig.sharing_queue, serde_json::to_vec(&SharingTask::ShareToSquad{
            user_id,
            match_uuid: match_uuid.clone(),
            game,
            squad_id,
            conn: conn.clone(),
            parent_connection_id,
        })?, RABBITMQ_DEFAULT_PRIORITY, MAX_AGE_SECONDS).await;
        Ok(())
    }

    pub async fn handle_vod_share_to_profile(&self, tx : &mut Transaction<'_, Postgres>, user_id: i64, vod: &VodAssociation) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.user_profile_vods (
                user_id,
                video_uuid
            ) VALUES (
                $1,
                $2
            )
            ON CONFLICT DO NOTHING
            ",
            user_id,
            &vod.video_uuid,
        )
            .execute(&mut *tx)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl RabbitMqListener for SharingRabbitmqInterface {
    async fn handle(&self, data: &[u8], _queue: &str) -> Result<(), SquadOvError> {
        log::info!("Handle Sharing Task: {}", std::str::from_utf8(data).unwrap_or("failure"));
        let task: SharingTask = serde_json::from_slice(data)?;
        match task {
            SharingTask::ShareToSquad{user_id, match_uuid, game, squad_id, conn, parent_connection_id} => {
                let mut tx = self.db.begin().await?;
                self.handle_vod_share_to_squad(&mut tx, user_id, &match_uuid, game, squad_id, &conn, parent_connection_id).await?;
                tx.commit().await?;
            },
        };
        Ok(())
    }
}