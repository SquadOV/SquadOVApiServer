use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    SquadOvWowRelease,
    games,
    WoWCharacter,
    WowFullCharacter,
    WowCovenant,
    WowItem,
    WoWCharacterUserAssociation,
};
use uuid::Uuid;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct WowCharacterDataInput {
    character_name: String,
    character_guid: String,
}

#[derive(Deserialize)]
pub struct WowCharacterPath {
    character_guid: String,
}

fn compute_wow_character_ilvl(items: &[i32]) -> i32 {
    let mut relevant_ilvls: Vec<i32> = items.iter().map(|x| { *x }).collect();

    if relevant_ilvls.len() == 18 {
        // We need to filter out shirts and tabards from the ilvl of the character.
        // In the case where the character has a 2-handed weapon equipped, that weapon needs to
        // count for double. Right now we have no way of determining the type of any particular item
        // so we do our best guesses on how to best filter this stuff.
        // There's 18 item slots and item index 15 is the primary weapon and index 16 is the off-hand weapon.
        // If the off-hand weapon has an ilvl of 0 then we assume that the user is using a two-handed.
        if relevant_ilvls[15] > 0 && relevant_ilvls[16] == 0 {
            relevant_ilvls[15] = relevant_ilvls[15] * 2;
        }
    }

    let relevant_ilvls: Vec<i32> = relevant_ilvls.into_iter().filter(|x| {
        *x > 1
    }).collect();
    
    (relevant_ilvls.iter().sum::<i32>() as f32 / 16.0).floor() as i32
}

impl api::ApiApplication {
    pub async fn list_wow_characters_for_match(&self, match_uuid: &Uuid, user_id: i64) -> Result<Vec<WoWCharacter>, SquadOvError> {
        // So for this it's gonna be slightly tricky as we won't necessarily have combatant info in this match since WoW can be f'd up sometimes.
        // In that case, we want to fallback to trying to find the latest combatant info. So to do this all efficiently, we just look up the guids
        // of players w/ combatant info in this match. If that's empty, then we assume the worst, and look for players with the player flag set.
        let combatant_guids = sqlx::query!(
            "
            SELECT wcp.unit_guid
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.view_id = wmv.id
            WHERE wmv.match_uuid = $1
                AND wmv.user_id = $2
                AND wcp.has_combatant_info
            ",
            match_uuid,
            user_id,
        )
            .fetch_all(&*self.pool)
            .await?;

        if combatant_guids.is_empty() {
            // If combatant guids don't exist, then combatant info does not exist in this match and we thus
            // will need to fill in filler data for characters.
            Ok(
                sqlx::query!(
                    r#"
                    SELECT
                        wcp.unit_guid,
                        COALESCE(wcp.unit_name, '') AS "unit_name!",
                        wcp.class_id
                    FROM squadov.wow_match_view AS wmv
                    INNER JOIN squadov.wow_match_view_character_presence AS wcp
                        ON wcp.view_id = wmv.id
                    WHERE wmv.match_uuid = $1
                        AND wmv.user_id = $2
                        AND (wcp.flags & x'100'::BIGINT) > 0
                        AND (wcp.flags & x'400'::BIGINT) > 0
                    "#,
                    match_uuid,
                    user_id
                )
                    .fetch_all(&*self.pool)
                    .await?
                    .into_iter()
                    .map(|x| {
                        WoWCharacter {
                            guid: x.unit_guid,
                            name: x.unit_name,
                            ilvl: -1,
                            spec_id: -1,
                            team: -1,
                            rating: -1,
                            class_id: x.class_id.map(|x| { x as i64 }),
                        }
                    })
                    .collect()
            )
        } else {
            // If combatant guids exist, then we know we can look up combatant info in this match.
            // However, in the case of keystones, there could be multiple combatant info logs within the
            // same match. In that case we'll just take the first one since we'll just assume they're
            // all the same.
            Ok(
                sqlx::query!(
                    r#"
                    SELECT DISTINCT
                        wcp.unit_guid AS "guid",
                        COALESCE(wcp.unit_name, '') AS "name!",
                        COALESCE(ARRAY_AGG(wci.ilvl ORDER BY wci.idx ASC), ARRAY[]::INTEGER[]) AS "items!",
                        wvc.spec_id,
                        wvc.team,
                        wvc.rating,
                        wvc.class_id
                    FROM squadov.wow_match_view AS wmv
                    INNER JOIN squadov.wow_match_view_character_presence AS wcp
                        ON wcp.view_id = wmv.id
                            AND wcp.has_combatant_info
                    INNER JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    LEFT JOIN squadov.wow_match_view_combatant_items AS wci
                        ON wci.event_id = wvc.event_id
                    WHERE wmv.match_uuid = $1
                        AND wmv.user_id = $2
                    GROUP BY wcp.unit_guid, wcp.unit_name, wvc.spec_id, wvc.team, wvc.event_id, wvc.rating, wvc.class_id
                    "#,
                    match_uuid,
                    user_id
                )
                    .fetch_all(&*self.heavy_pool)
                    .await?
                    .into_iter()
                    .map(|x| {
                        WoWCharacter {
                            guid: x.guid,
                            name: x.name,
                            ilvl: compute_wow_character_ilvl(&x.items),
                            spec_id: x.spec_id,
                            team: x.team,
                            rating: x.rating,
                            class_id: x.class_id,
                        }
                    })
                    .collect()
            )
        }

    }

    async fn list_wow_characters_for_user(&self, user_id: i64, release: SquadOvWowRelease) -> Result<Vec<WoWCharacter>, SquadOvError> {
        // We can afford to only list combatant info-validated here as we expect the issue where combatant info doesn't show up
        // to be a rare occurence.
        Ok(
            sqlx::query!(
                r#"
                SELECT
                    wucc.unit_guid AS "guid",
                    COALESCE(wucc.unit_name, '') AS "name!",
                    COALESCE(wucc.items, ARRAY[]::INTEGER[]) AS "items!",
                    COALESCE(wucc.spec_id, -1) AS "spec_id!",
                    wucc.class_id
                FROM squadov.wow_user_character_cache AS wucc
                WHERE wucc.user_id = $1
                    AND wucc.build_version SIMILAR TO $2::VARCHAR
                "#,
                user_id,
                games::wow_release_to_db_build_expression(release),
            )
                .fetch_all(&*self.heavy_pool)
                .await?
                .into_iter()
                .map(|x| {
                    WoWCharacter {
                        guid: x.guid,
                        name: x.name,
                        ilvl: compute_wow_character_ilvl(&x.items),
                        spec_id: x.spec_id,
                        team: 0,
                        rating: 0,
                        class_id: x.class_id.map(|x| { x as i64 } ),
                    }
                })
                .collect()
        )
    }

    async fn list_wow_characters_association_for_squad_in_match(&self, match_uuid: &Uuid, view_user_id: i64, request_user_id: i64) -> Result<Vec<WoWCharacterUserAssociation>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                WoWCharacterUserAssociation,
                r#"
                SELECT wucc.user_id, wucc.unit_guid AS "guid"
                FROM squadov.wow_match_view AS wmv
                INNER JOIN squadov.wow_match_view_character_presence AS wcp
                    ON wcp.view_id = wmv.id
                INNER JOIN squadov.wow_user_character_cache AS wucc
                    ON wucc.unit_guid = wcp.unit_guid
                INNER JOIN (
                    SELECT $3
                    UNION
                    SELECT ora.user_id
                    FROM squadov.users AS u
                    LEFT JOIN squadov.squad_role_assignments AS sra
                        ON sra.user_id = u.id
                    LEFT JOIN squadov.squad_role_assignments AS ora
                        ON ora.squad_id = sra.squad_id
                    WHERE u.id = $3
                ) AS squ (user_id)
                    ON squ.user_id = wucc.user_id
                WHERE wmv.match_uuid = $1
                    AND wmv.user_id = $2
                "#,
                match_uuid,
                view_user_id,
                request_user_id,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn get_wow_realm_region(&self, realm_id: i64) -> Result<String, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                SELECT region AS "region!"
                FROM squadov.wow_realms
                WHERE id = $1
                UNION
                SELECT region AS "region!"
                FROM squadov.wow_connected_realms
                WHERE id = $1
                "#,
                realm_id,
            )
                .fetch_one(&*self.pool)
                .await?
                .region
        )
    }

    async fn get_wow_realm_slug(&self, realm_id: i64, name: &str) -> Result<String, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                WITH realms AS (
                    SELECT *
                    FROM squadov.wow_realms
                    WHERE id = $1
                    UNION
                    SELECT wr.*
                    FROM squadov.wow_connected_realms AS wcr
                    INNER JOIN squadov.wow_connected_realm_members AS crm
                        ON crm.connected_realm_id = wcr.id
                    INNER JOIN squadov.wow_realms AS wr
                        ON wr.id = crm.realm_id
                    WHERE wcr.id = $1
                )
                SELECT slug AS "slug!"
                FROM realms
                ORDER BY LEVENSHTEIN(name, $2) ASC
                LIMIT 1
                "#,
                realm_id,
                name,
            )
                .fetch_one(&*self.pool)
                .await?
                .slug
        )
    }

    async fn get_wow_character_covenant(&self, view_uuid: &Uuid, guid: &str) -> Result<Option<WowCovenant>, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT DISTINCT
                    wcc.covenant_id,
                    wcc.soulbind_id,
                    wcc.soulbind_traits,
                    wcc.conduit_item_ids,
                    wcc.conduit_item_ilvls
                FROM squadov.wow_match_view_character_presence AS wcp
                INNER JOIN squadov.wow_match_view_combatant_covenants AS wcc
                    ON wcc.character_id = wcp.character_id
                WHERE wcp.view_id = $1
                    AND wcp.unit_guid = $2
                ",
                view_uuid,
                guid
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    WowCovenant {
                        covenant_id: x.covenant_id,
                        soulbind_id: x.soulbind_id,
                        soulbind_traits: x.soulbind_traits,
                        conduits: x.conduit_item_ids.iter().zip(x.conduit_item_ilvls.iter()).map(|(item_id, ilvl)| {
                            WowItem{
                                item_id: *item_id,
                                ilvl: *ilvl,
                            }
                        }).collect(),
                    }
                })
        )
    }

    async fn get_wow_full_character(&self, view_uuid: &Uuid, guid: &str) -> Result<WowFullCharacter, SquadOvError> {
        let items: Vec<WowItem> = sqlx::query_as!(
            WowItem,
            r#"
            SELECT
                MAX(wci.item_id) AS "item_id!",
                MAX(wci.ilvl) AS "ilvl!"
            FROM squadov.wow_match_view_character_presence AS wcp
            LEFT JOIN squadov.wow_match_view_combatant_items AS wci
                ON wci.character_id = wcp.character_id
            WHERE wcp.view_id = $1
                AND wcp.unit_guid = $2
            GROUP BY wcp.unit_guid, wci.idx
            ORDER BY wci.idx ASC
            "#,
            view_uuid,
            guid,
        )
            .fetch_all(&*self.pool)
            .await?;

        let talents = sqlx::query!(
            r#"
            SELECT DISTINCT wct.talent_id, wct.is_pvp
            FROM squadov.wow_match_view_character_presence AS wcp
            INNER JOIN squadov.wow_match_view_combatant_talents AS wct
                ON wct.character_id = wcp.character_id
            WHERE wcp.view_id = $1
                AND wcp.unit_guid = $2
            "#,
            view_uuid,
            guid,
        )
            .fetch_all(&*self.pool)
            .await?;
        
        Ok(WowFullCharacter {
            items,
            covenant: self.get_wow_character_covenant(view_uuid, guid).await?,
            talents: talents.iter().filter(|x| { !x.is_pvp }).map(|x| { x.talent_id }).collect(),
            pvp_talents: talents.iter().filter(|x| { x.is_pvp }).map(|x| { x.talent_id }).collect(),
        })
    }
}

#[derive(Deserialize)]
pub struct CharactersForUserQuery {
    release: SquadOvWowRelease
}

pub async fn list_wow_characters_for_user_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserPath>, query: web::Query<CharactersForUserQuery>) -> Result<HttpResponse, SquadOvError> {
    let chars = app.list_wow_characters_for_user(path.user_id, query.release).await?;
    Ok(HttpResponse::Ok().json(chars))
}

pub async fn list_wow_characters_for_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>) -> Result<HttpResponse, SquadOvError> {
    let chars = app.list_wow_characters_for_match(&path.match_uuid, path.user_id).await?;
    Ok(HttpResponse::Ok().json(chars))
}

pub async fn list_wow_characters_association_for_squad_in_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let chars = app.list_wow_characters_association_for_squad_in_match(&path.match_uuid, path.user_id, session.user.id).await?;
    Ok(HttpResponse::Ok().json(chars))
}

pub async fn get_wow_armory_link_for_character_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Query<WowCharacterDataInput>) -> Result<HttpResponse, SquadOvError> {
    // Get character name for this GUID which is composed of the CHARACTER NAME-SERVER NAME.
    // Next, obtain the region for the extracted server name.
    let name_parts: Vec<&str> = data.character_name.split("-").into_iter().collect();
    if name_parts.len() != 2 {
        return Err(SquadOvError::InternalError(format!("Unexpected WoW name: {}", &data.character_name)));
    }

    let char_name = name_parts[0];
    let server_name = name_parts[1];

    let guid_parts: Vec<&str> = data.character_guid.split("-").into_iter().collect();
    if guid_parts.len() != 3 {
        return Err(SquadOvError::InternalError(format!("Unexpected WoW GUID: {}", &data.character_guid)));
    }
    let region_id = guid_parts[1].parse::<i64>()?;
    let region = app.get_wow_realm_region(region_id).await?;

    // The realm ID in the GUID can be a connected realm or an actual realm and the server name that's passed
    // to us from the user IS NOT THE RIGHT SLUG. What we need to do is to find the realm whose name is most similar
    // to the passed in server name and use that to generate the armory link.
    let slug = app.get_wow_realm_slug(region_id, &server_name).await?;

    // Finally compose the WoW armory link: 
    // https://worldofwarcraft.com/en-us/character/REGION/SERVER NAME/CHARACTER NAME
    Ok(HttpResponse::Ok().json(
        format!(
            "https://worldofwarcraft.com/en-us/character/{region}/{server}/{character}",
            region=region,
            server=slug,
            character=char_name,
        )
    ))
}

pub async fn get_full_wow_character_for_match_handler(app : web::Data<Arc<api::ApiApplication>>, match_path: web::Path<super::WoWUserMatchPath>, char_path: web::Path<WowCharacterPath>) -> Result<HttpResponse, SquadOvError> {
    let view_uuid = app.get_wow_match_view_for_user_match(match_path.user_id, &match_path.match_uuid).await?.ok_or(SquadOvError::NotFound)?;
    Ok(HttpResponse::Ok().json(app.get_wow_full_character(&view_uuid, &char_path.character_guid).await?))
}