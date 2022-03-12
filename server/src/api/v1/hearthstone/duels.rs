use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use chrono::{DateTime, Utc};
use squadov_common::SquadOvError;
use squadov_common::hearthstone::{HearthstoneDuelRun, GameType};
use uuid::Uuid;
use sqlx::{Transaction, Postgres};
use std::sync::Arc;
use std::convert::TryFrom;

impl api::ApiApplication {
    pub async fn create_hearthstone_duels_run(&self, tx: &mut Transaction<'_, Postgres>, match_uuid: &Uuid, user_id: i64, start_time: &DateTime<Utc>) -> Result<Uuid, SquadOvError> {
        let mc = self.create_new_match_collection(tx).await?;
        sqlx::query!(
            "
            INSERT INTO squadov.hearthstone_duels (
                collection_uuid,
                user_id,
                deck_id,
                creation_time
            )
            SELECT $1, $2, hdv.deck_id, $4
            FROM squadov.hearthstone_deck_versions AS hdv
            INNER JOIN squadov.hearthstone_match_user_deck AS hmud
                ON hmud.deck_version_id = hdv.version_id
            WHERE hmud.user_id = $2 AND hmud.match_uuid = $3
            ",
            mc.uuid,
            user_id,
            match_uuid,
            start_time,
        )
            .execute(tx)
            .await?;
        Ok(mc.uuid)
    }

    async fn list_duel_runs_for_user(&self, user_id: i64, start: i64, end: i64) -> Result<Vec<Uuid>, SquadOvError> {
        Ok(sqlx::query_scalar(
            "
            SELECT collection_uuid
            FROM squadov.hearthstone_duels
            WHERE user_id = $1
            ORDER BY creation_time DESC
            LIMIT $2 OFFSET $3
            ",
        )
            .bind(user_id)
            .bind(end - start)
            .bind(start)
            .fetch_all(&*self.pool)
            .await?
        )
    }

    async fn list_matches_for_duel_run(&self, collection_uuid: &Uuid, user_id: i64, filters: &super::HearthstoneListQuery) -> Result<Vec<Uuid>, SquadOvError> {
        Ok(sqlx::query!(
            "
            SELECT mmc.match_uuid
            FROM squadov.match_to_match_collection AS mmc
            INNER JOIN squadov.hearthstone_duels AS had
                ON had.collection_uuid = mmc.collection_uuid AND had.user_id = $2
            INNER JOIN squadov.hearthstone_matches AS hm
                ON hm.match_uuid = mmc.match_uuid
            CROSS JOIN (
                SELECT *
                FROM squadov.users
                WHERE id = $2
            ) AS u
            LEFT JOIN squadov.vods AS v
                ON v.match_uuid = mmc.match_uuid
                    AND v.user_uuid = u.uuid
                    AND v.is_clip = FALSE
            WHERE mmc.collection_uuid = $1
                AND (NOT $3::BOOLEAN OR v.video_uuid IS NOT NULL)
            ORDER BY hm.match_time DESC
            ",
            collection_uuid,
            user_id,
            filters.has_vod.unwrap_or(false),
        )
            .fetch_all(&*self.pool)
            .await?
            .into_iter()
            .map(|x| {
                x.match_uuid
            })
            .collect()
        )
    }

    async fn get_hearthstone_duel_run(&self, collection_uuid: &Uuid, user_id: i64)  -> Result<HearthstoneDuelRun, SquadOvError> {
        let basic_data = sqlx::query!(
            r#"
            WITH duel_matches AS (
                SELECT hmm.*
                FROM squadov.hearthstone_match_metadata AS hmm
                INNER JOIN squadov.hearthstone_match_view AS hmv
                    ON hmv.view_uuid = hmm.view_uuid
                INNER JOIN squadov.match_to_match_collection AS mmc
                    ON mmc.match_uuid = hmv.match_uuid
                WHERE mmc.collection_uuid = $1 AND hmv.user_id = $2
            ), duel_ratings AS (
                SELECT MAX(duels_casual_rating) AS "casual", MAX(duels_heroic_rating) AS "heroic"
                FROM duel_matches AS dm
                INNER JOIN squadov.hearthstone_match_players AS hmp
                    ON hmp.view_uuid = dm.view_uuid
                WHERE hmp.user_id = $2
                GROUP BY hmp.user_id
            )
            SELECT
                collection_uuid AS "duel_uuid",
                deck_id,
                creation_time AS "timestamp",
                (
                    SELECT COUNT(dm.view_uuid)
                    FROM duel_matches AS dm
                    INNER JOIN squadov.hearthstone_match_players AS hmp
                        ON hmp.view_uuid = dm.view_uuid AND hmp.player_match_id = dm.match_winner_player_id
                    WHERE hmp.user_id = $2
                ) AS "wins!",
                (
                    SELECT COUNT(dm.view_uuid)
                    FROM duel_matches AS dm
                    INNER JOIN squadov.hearthstone_match_players AS hmp
                        ON hmp.view_uuid = dm.view_uuid AND hmp.player_match_id = dm.match_winner_player_id
                    WHERE hmp.user_id != $2
                ) AS "loss!",
                (
                    SELECT casual
                    FROM duel_ratings
                    LIMIT 1
                ) AS "casual_rating",
                (
                    SELECT heroic
                    FROM duel_ratings
                    LIMIT 1
                ) AS "heroic_rating",
                (
                    SELECT DISTINCT hmm.game_type
                    FROM duel_matches AS dm
                    INNER JOIN squadov.hearthstone_match_metadata AS hmm
                        ON hmm.view_uuid = dm.view_uuid
                    LIMIT 1
                ) AS "game_type!"
            FROM squadov.hearthstone_duels
            WHERE collection_uuid = $1 AND user_id = $2
            "#,
            collection_uuid,
            user_id
        )
            .fetch_one(&*self.pool)
            .await?;

        let deck = self.get_latest_hearthstone_deck(basic_data.deck_id, user_id).await?;

        // Note that for duels the hero card stored in the deck isn't what we want to display to the user
        // as to what the duel run's hero is. Instead, look at the latest game and grab the hero entity from that.
        // If there's no matches (which technically is impossible since we only create a duel run upon uploading a match)
        // then we fall back to the deck's hero card.
        let matches = self.list_matches_for_duel_run(collection_uuid, user_id, &super::HearthstoneListQuery::default()).await?;
        let hero_card = if matches.len() > 0 {
            let snapshot_ids = self.get_hearthstone_snapshots_for_match(&matches[0], user_id).await?;
            let player_entity = match snapshot_ids.last() {
                Some(x) => Some(self.get_player_hero_entity_from_hearthstone_snapshot(x, user_id).await?),
                None => None
            };

            if player_entity.is_some() {
                player_entity.unwrap().card_id()
            } else {
                None
            }
        } else {
            None
        };

        let game_type = GameType::try_from(basic_data.game_type)?;
        let rating = if game_type == GameType::PvpDr {
            basic_data.casual_rating
        } else {
            basic_data.heroic_rating
        };

        Ok(HearthstoneDuelRun{
            duel_uuid: basic_data.duel_uuid,
            hero_card,
            deck,
            wins: basic_data.wins,
            loss: basic_data.loss,
            timestamp: basic_data.timestamp,
            rating
        })
    }
}

pub async fn list_duel_runs_for_user_handler(data : web::Path<super::HearthstoneUserMatchInput>, query: web::Query<api::PaginationParameters>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let query = query.into_inner();
    let runs = app.list_duel_runs_for_user(
        data.user_id,
        query.start,
        query.end,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = runs.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(runs, &req, &query, expected_total == got_total)?)) 
}

pub async fn list_matches_for_duel_run_handler(data : web::Path<super::HearthstoneCollectionGetInput>, app : web::Data<Arc<api::ApiApplication>>, filters: web::Json<super::HearthstoneListQuery>) -> Result<HttpResponse, SquadOvError> {
    let matches = app.list_matches_for_duel_run(&data.collection_uuid, data.user_id, &filters).await?;
    Ok(HttpResponse::Ok().json(&matches))
}

pub async fn get_hearthstone_duel_run_handler(data : web::Path<super::HearthstoneCollectionGetInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let duel_run = app.get_hearthstone_duel_run(&data.collection_uuid, data.user_id).await?;
    Ok(HttpResponse::Ok().json(&duel_run))
}