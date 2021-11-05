use actix_web::{web, HttpResponse};
use crate::api;
use squadov_common::SquadOvError;
use squadov_common::hearthstone::{HearthstoneCardMetadata, HearthstoneBattlegroundsCardMetadata};
use std::sync::Arc;
use serde::{Deserialize};

#[derive(Deserialize)]
pub struct TavernLevelInput {
    tavern_level: i32
}

impl api::ApiApplication {
    pub async fn bulk_get_hearthstone_cards_metadata(&self, card_ids: &[String]) -> Result<Vec<HearthstoneCardMetadata>, SquadOvError> {
        Ok(sqlx::query_as!(
            HearthstoneCardMetadata,
            r#"
            SELECT
                hc.card_id AS "card_id!",
                hcn.string AS "name!",
                hct_cost.val AS "cost!",
                hct_rare.val AS "rarity!"
            FROM squadov.hearthstone_cards AS hc
            INNER JOIN squadov.hearthstone_card_names AS hcn
                ON hcn.card_id = hc.card_id AND hcn.locale = 'en_US'
            INNER JOIN squadov.hearthstone_card_tags AS hct_cost
                ON hct_cost.card_id = hc.card_id AND hct_cost.tag = 48
            INNER JOIN squadov.hearthstone_card_tags AS hct_rare
                ON hct_rare.card_id = hc.card_id AND hct_rare.tag = 203
            WHERE hc.card_id = any($1)
            "#,
            card_ids
        )
            .fetch_all(&*self.pool)
            .await?)
    }

    pub async fn get_hearthstone_cards_for_battlegrounds_tavern_level(&self, level: i32) -> Result<Vec<HearthstoneBattlegroundsCardMetadata>, SquadOvError> {
        let tavern_cards = sqlx::query!(
            r#"
            SELECT
                hc.card_id,
                hcn.string AS "name",
                hct_cost.val AS "cost",
                hct_race.val AS "race?"
            FROM squadov.hearthstone_cards AS hc
            INNER JOIN squadov.hearthstone_card_names AS hcn
                ON hcn.card_id = hc.card_id AND hcn.locale = 'en_US'
            INNER JOIN squadov.hearthstone_card_tags AS hct_cost
                ON hct_cost.card_id = hc.card_id AND hct_cost.tag = 48
            INNER JOIN squadov.hearthstone_card_tags AS hct_bacon
                ON hct_bacon.card_id = hc.card_id AND hct_bacon.tag = 1456
            INNER JOIN squadov.hearthstone_card_tags AS hct_tech
                ON hct_tech.card_id = hc.card_id AND hct_tech.tag = 1440
            LEFT JOIN squadov.hearthstone_card_tags AS hct_race
                ON hct_race.card_id = hc.card_id AND hct_race.tag = 200
            WHERE hct_tech.val = $1 AND hct_bacon.val = 1
            "#,
            level
        )
            .fetch_all(&*self.pool)
            .await?;

        Ok(tavern_cards.into_iter().map(|x| {
            HearthstoneBattlegroundsCardMetadata{
                base: HearthstoneCardMetadata {
                    card_id: x.card_id,
                    name: x.name,
                    cost: x.cost,
                    // Card rarity doesn't matter for battlegrounds. In fact if we do query for it
                    // with an inner join we're going to miss cards that don't actually have a rarity.
                    rarity: 0,
                },
                tavern_level: level,
                card_race: x.race
            }
        }).collect())
    }
}

pub async fn bulk_get_hearthstone_cards_metadata_handler(app : web::Data<Arc<api::ApiApplication>>, card_ids : web::Json<Vec<String>>) -> Result<HttpResponse, SquadOvError> {
    let metadata = app.bulk_get_hearthstone_cards_metadata(&card_ids).await?;
    Ok(HttpResponse::Ok().json(&metadata))
}

pub async fn get_battleground_tavern_level_cards_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Path<TavernLevelInput>) -> Result<HttpResponse, SquadOvError> {
    let cards = app.get_hearthstone_cards_for_battlegrounds_tavern_level(data.tavern_level).await?;
    Ok(HttpResponse::Ok().json(&cards))
}