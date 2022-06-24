use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use crate::api;
use crate::api::auth::SquadOVSession;
use std::sync::Arc;
use squadov_common::{
    SquadOvError,
    SquadOvWowRelease,
    WoWCharacterUserAssociation,
    wow::{
        characters::{
            self,
            WowFullCharacter,
        },
        matches,
        reports::{
            WowReportTypes,
            characters::{
                WowCombatantReport,
            },
        },
        WoWCharacter,
    },
};
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

impl api::ApiApplication {
    async fn list_wow_characters_association_for_squad_from_guids(&self, guids: &[String], request_user_id: i64) -> Result<Vec<WoWCharacterUserAssociation>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                WoWCharacterUserAssociation,
                r#"
                SELECT wucc.user_id AS "user_id!", wucc.unit_guid AS "guid!"
                FROM  squadov.wow_user_character_cache AS wucc
                INNER JOIN (
                    SELECT $2
                    UNION
                    SELECT ora.user_id
                    FROM squadov.users AS u
                    LEFT JOIN squadov.squad_role_assignments AS sra
                        ON sra.user_id = u.id
                    LEFT JOIN squadov.squad_role_assignments AS ora
                        ON ora.squad_id = sra.squad_id
                    WHERE u.id = $2
                ) AS squ (user_id)
                    ON squ.user_id = wucc.user_id
                WHERE wucc.unit_guid = ANY($1)
                "#,
                guids,
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
                ORDER BY LEVENSHTEIN(name, $2::VARCHAR) ASC
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
}

#[derive(Deserialize)]
pub struct CharactersForUserQuery {
    release: SquadOvWowRelease
}

pub async fn list_wow_characters_for_user_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserPath>, query: web::Query<CharactersForUserQuery>) -> Result<HttpResponse, SquadOvError> {
    let chars = characters::list_wow_characters_for_user(&*app.pool, path.user_id, Some(query.release)).await?;
    Ok(HttpResponse::Ok().json(chars))
}

pub async fn list_wow_characters_for_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>) -> Result<HttpResponse, SquadOvError> {
    let chars = if let Some(combat_log_partition_id) = matches::get_generic_wow_match_view_from_match_user(&*app.pool, &path.match_uuid, path.user_id).await?.combat_log_partition_id {
        let chars: Vec<WoWCharacter> = app.cl_itf.get_report_avro::<WowCombatantReport>(&combat_log_partition_id, WowReportTypes::MatchCombatants as i32, "combatants.avro").await?.into_iter().map(|x| {
            x.into()
        }).collect();

        if chars.iter().any(|x| { x.ilvl > 0 }) {
            // If one combatant has and ilvl then that means we need to filter all the other useless chars that don't have an ilvl.
            chars.into_iter().filter(|x| { x.ilvl > 0 }).collect()
        } else {
            chars
        }
    } else {
        return Err(SquadOvError::BadRequest);
    };

    Ok(HttpResponse::Ok().json(chars))
}

pub async fn list_wow_characters_association_for_squad_in_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    
    Ok(HttpResponse::Ok().json(if let Some(combat_log_partition_id) = matches::get_generic_wow_match_view_from_match_user(&*app.pool, &path.match_uuid, path.user_id).await?.combat_log_partition_id {
        let combatants: Vec<String> = app.cl_itf.get_report_avro::<WowCombatantReport>(&combat_log_partition_id, WowReportTypes::MatchCombatants as i32, "combatants.avro").await?.into_iter().map(|x| {
            x.unit_guid
        }).collect();
        app.list_wow_characters_association_for_squad_from_guids(&combatants, session.user.id).await?
    } else {
        return Err(SquadOvError::BadRequest);
    }))
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
    let match_view = squadov_common::wow::matches::get_generic_wow_match_view_from_match_user(&*app.pool, &match_path.match_uuid, match_path.user_id).await?;
    Ok(HttpResponse::Ok().json(if let Some(combat_log_partition_id) = match_view.combat_log_partition_id.as_ref() {
        app.cl_itf.get_report_json::<WowFullCharacter>(&combat_log_partition_id, WowReportTypes::CharacterLoadout as i32, &format!("{}.json", &char_path.character_guid)).await?
    } else {
        return Err(SquadOvError::BadRequest);
    }))
}