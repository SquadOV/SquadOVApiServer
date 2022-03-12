use squadov_common::SquadOvError;
use crate::api;
use crate::api::v1;
use actix_web::{web, HttpResponse, HttpRequest};
use serde::{Deserialize};
use sqlx::{Transaction, Postgres};
use std::sync::Arc;
use uuid::Uuid;
use chrono::{DateTime,Utc};
use squadov_common::hearthstone::{HearthstoneArenaRun, HearthstoneDeck};

#[derive(Deserialize)]
pub struct CreateHearthstoneArenaDeckInput {
    #[serde(rename = "deckId")]
    deck_id: i64,
    tm: DateTime<Utc>
}

#[derive(Deserialize)]
pub struct AddHearthstoneArenaCardInput {
    #[serde(rename = "cardId")]
    card_id: String,
    tm: DateTime<Utc>
}

impl api::ApiApplication {
    async fn check_if_arena_draft_exists(&self, deck_id: i64) -> Result<Option<v1::MatchCollection>, squadov_common::SquadOvError> {
        Ok(sqlx::query_as!(
            v1::MatchCollection,
            "
            SELECT mc.*
            FROM squadov.hearthstone_arena_drafts AS had
            INNER JOIN squadov.match_collections AS mc
                ON mc.uuid = had.collection_uuid
            WHERE had.draft_deck_id = $1
            ",
            deck_id
        )
            .fetch_optional(&*self.pool)
            .await?)
    }

    async fn create_or_retrieve_arena_draft_for_user(&self, tx : &mut Transaction<'_, Postgres>, user_id: i64, deck_id: i64, tm: &DateTime<Utc>) -> Result<Uuid, SquadOvError> {
        Ok(match self.check_if_arena_draft_exists(deck_id).await? {
            Some(x) => x.uuid,
            None => {
                let mc = self.create_new_match_collection(tx).await?;
                // This is needed to let the deck ID foreign key constraint pass while the user hasn't yet drafted the deck yet.
                self.create_empty_hearthstone_deck(tx, deck_id, user_id).await?;
                sqlx::query!(
                    "
                    INSERT INTO squadov.hearthstone_arena_drafts (
                        collection_uuid,
                        user_id,
                        draft_deck_id,
                        creation_time
                    )
                    VALUES (
                        $1,
                        $2,
                        $3,
                        $4
                    )
                    ",
                    mc.uuid,
                    user_id,
                    deck_id,
                    tm,

                )
                    .execute(tx)
                    .await?;
                mc.uuid
            }
        })
    }

    async fn add_hearthstone_card_to_arena_deck(&self, tx : &mut Transaction<'_, Postgres>, collection_uuid: &Uuid, user_id: i64, card_id: &str, tm: &DateTime<Utc>) -> Result<(), SquadOvError> {
        sqlx::query!(
            "
            INSERT INTO squadov.hearthstone_arena_deck_slots(
                deck_id,
                card_id,
                selection_time
            )
            SELECT
                had.draft_deck_id,
                $2,
                $3
            FROM squadov.hearthstone_arena_drafts AS had
            WHERE had.collection_uuid = $1
                AND had.user_id = $4
            ",
            collection_uuid,
            card_id,
            tm,
            user_id,
        )
            .execute(tx)
            .await?;
        Ok(())
    }

    async fn list_arena_runs_for_user(&self, user_id: i64, start: i64, end: i64) -> Result<Vec<Uuid>, SquadOvError> {
        Ok(sqlx::query_scalar(
            "
            SELECT collection_uuid
            FROM squadov.hearthstone_arena_drafts
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

    async fn list_matches_for_arena_run(&self, collection_uuid: &Uuid, user_id: i64, filters: &super::HearthstoneListQuery) -> Result<Vec<Uuid>, SquadOvError> {
        Ok(sqlx::query!(
            "
            SELECT mmc.match_uuid
            FROM squadov.match_to_match_collection AS mmc
            INNER JOIN squadov.hearthstone_arena_drafts AS had
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

    async fn get_hearthstone_arena_run(&self, collection_uuid: &Uuid, user_id: i64)  -> Result<HearthstoneArenaRun, SquadOvError> {
        // We need to grab basic information about the arena run: namely the deck, win/loss, and time for us to
        // successfully display basic information about the run. To get the deck, we use the deck ID and query the
        // database (straight-forward). Time is stored  in the same row as the arena draft in the database as well.
        // The only tricky thing is figuring out how to get the win/loss. To do this, we assume that if there are 0
        // games played, then the user is at 0-0 (win-loss). To get the *FINAL* win-loss, then we have to look at the
        // last game played to get W-L and depending on whether the last game is a win or a loss for the player in question
        // return (W+1)-L or W-(L+1).
        let basic_data = sqlx::query!(
            "
            SELECT draft_deck_id, creation_time
            FROM squadov.hearthstone_arena_drafts
            WHERE collection_uuid = $1 AND user_id = $2
            ",
            collection_uuid,
            user_id
        )
            .fetch_one(&*self.pool)
            .await?;

        let mut wins = 0;
        let mut loss = 0;

        let matches = self.list_matches_for_arena_run(collection_uuid, user_id, &super::HearthstoneListQuery::default()).await?;
        if !matches.is_empty() {
            let match_players = self.get_hearthstone_players_for_match(&matches[0], user_id).await?;
            for (_, player) in match_players {
                if player.local {
                    wins = player.arena_wins;
                    loss = player.arena_loss;
                }
            }

            let win = self.did_user_win_hearthstone_match(&matches[0], user_id).await?;
            if win {
                wins += 1
            } else {
                loss += 1
            }
        }

        Ok(HearthstoneArenaRun{
            arena_uuid: collection_uuid.clone(),
            deck: self.get_latest_hearthstone_deck(basic_data.draft_deck_id, user_id).await?,
            wins,
            loss,
            timestamp: basic_data.creation_time
        })
    }
}

pub async fn create_or_retrieve_arena_draft_for_user_handler(data : web::Path<super::HearthstoneUserMatchInput>, body : web::Json<CreateHearthstoneArenaDeckInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    let uuid = app.create_or_retrieve_arena_draft_for_user(&mut tx, data.user_id, body.deck_id, &body.tm).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(&uuid))
}

pub async fn add_hearthstone_card_to_arena_deck_handler(data : web::Path<super::HearthstoneCollectionGetInput>, body : web::Json<AddHearthstoneArenaCardInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let mut tx = app.pool.begin().await?;
    app.add_hearthstone_card_to_arena_deck(&mut tx, &data.collection_uuid, data.user_id, &body.card_id, &body.tm).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}

pub async fn list_arena_runs_for_user_handler(data : web::Path<super::HearthstoneUserMatchInput>, query: web::Query<api::PaginationParameters>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let query = query.into_inner();
    let runs = app.list_arena_runs_for_user(
        data.user_id,
        query.start,
        query.end,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = runs.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(runs, &req, &query, expected_total == got_total)?)) 
}

pub async fn list_matches_for_arena_run_handler(data : web::Path<super::HearthstoneCollectionGetInput>, app : web::Data<Arc<api::ApiApplication>>, filters: web::Json<super::HearthstoneListQuery>) -> Result<HttpResponse, SquadOvError> {
    let matches = app.list_matches_for_arena_run(&data.collection_uuid, data.user_id, &filters).await?;
    Ok(HttpResponse::Ok().json(&matches))
}

pub async fn get_hearthstone_arena_run_handler(data : web::Path<super::HearthstoneCollectionGetInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let arena_run = app.get_hearthstone_arena_run(&data.collection_uuid, data.user_id).await?;
    Ok(HttpResponse::Ok().json(&arena_run))
}

pub async fn create_finished_arena_draft_deck_handler(path : web::Path<super::HearthstoneCollectionGetInput>, data : web::Json<HearthstoneDeck>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    // Verify that the deck ID matches what we expect first.
    let existing_deck = match app.get_hearthstone_deck_for_arena_run(&path.collection_uuid, path.user_id).await? {
        Some(x) => x,
        None => return Err(SquadOvError::NotFound)
    };

    if existing_deck.deck_id != data.deck_id {
        return Err(SquadOvError::BadRequest);
    }

    let mut tx = app.pool.begin().await?;
    app.store_hearthstone_deck(&mut tx, &data, path.user_id).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().finish())
}