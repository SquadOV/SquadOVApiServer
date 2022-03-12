use squadov_common::{
    riot::{
        db,
        games::ValorantMatchDto
    },
    SquadOvError,
};
use crate::api;
use crate::api::auth::SquadOVSession;
use crate::api::v1::GenericMatchPathInput;
use squadov_common::vod::VodAssociation;
use actix_web::{web, HttpResponse, HttpRequest, HttpMessage};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::vec::Vec;
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use std::iter::FromIterator;

#[derive(Deserialize)]
pub struct GetValorantMatchDetailsInput {
    match_uuid: Uuid,
}

#[derive(Deserialize)]
pub struct GetValorantPlayerMatchMetadataInput {
    match_uuid: Uuid,
    puuid: String
}

struct RawValorantPlayerMatchMetadata {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>
}

impl api::ApiApplication {
    async fn get_puuids_in_valorant_match_from_user_uuids(&self, match_uuid: &Uuid, uuids: &[Uuid]) -> Result<Vec<(Uuid, String)>, SquadOvError> {
        let raw = sqlx::query!(
            "
            SELECT u.uuid, vmp.puuid
            FROM squadov.valorant_match_players AS vmp
            INNER JOIN squadov.riot_account_links AS ral
                ON ral.puuid = vmp.puuid
            INNER JOIN squadov.users AS u
                ON u.id = ral.user_id
            WHERE vmp.match_uuid = $1 AND u.uuid = any($2)
            ",
            match_uuid,
            uuids,
        )
            .fetch_all(&*self.pool)
            .await?;
        Ok(raw.into_iter().map(|x| {
            (x.uuid, x.puuid)
        }).collect())
    }

    async fn get_valorant_player_match_metadata(&self, match_uuid: &Uuid, puuid: &str)  -> Result<Option<super::ValorantPlayerMatchMetadata>, squadov_common::SquadOvError> {
        match sqlx::query_as!(
            RawValorantPlayerMatchMetadata,
            r#"
            SELECT
                vpmm.start_time,
                vpmm.end_time
            FROM squadov.valorant_player_match_metadata AS vpmm
            WHERE vpmm.match_uuid = $1
                AND vpmm.puuid = $2
            "#,
            match_uuid,
            puuid
        )
            .fetch_optional(&*self.pool)
            .await?
        {
            Some(x) => Ok(Some(super::ValorantPlayerMatchMetadata{
                start_time: x.start_time,
                end_time: x.end_time,
                rounds: sqlx::query_as!(
                    super::ValorantPlayerRoundMetadata,
                    r#"
                    SELECT
                        vprm.round,
                        vprm.buy_time,
                        vprm.round_time
                    FROM squadov.valorant_player_round_metadata AS vprm
                    WHERE vprm.match_uuid = $1
                        AND vprm.puuid = $2
                    "#,
                    match_uuid,
                    puuid,
                )
                    .fetch_all(&*self.pool)
                    .await?,
            })),
            None => Ok(None)
        }
    }
}

#[derive(Serialize)]
pub struct ValorantMatchDetails {
    uuid: Uuid,
    data: ValorantMatchDto
}

pub async fn get_valorant_match_details_handler(data : web::Path<GetValorantMatchDetailsInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let match_data = db::get_valorant_match(&*app.pool, &data.match_uuid).await?;
    Ok(HttpResponse::Ok().json(ValorantMatchDetails{
        uuid: data.match_uuid.clone(),
        data: match_data,
    }))
}

pub async fn get_valorant_player_match_metadata_handler(data: web::Path<GetValorantPlayerMatchMetadataInput>, app : web::Data<Arc<api::ApiApplication>>) -> Result<HttpResponse, SquadOvError> {
    let metadata = match app.get_valorant_player_match_metadata(&data.match_uuid, &data.puuid).await? {
        Some(x) => x,
        None => return Err(squadov_common::SquadOvError::NotFound)
    };
    Ok(HttpResponse::Ok().json(&metadata))
}

#[derive(Serialize)]
struct ValorantUserAccessibleVodOutput {
    pub vods: Vec<VodAssociation>,
    #[serde(rename="userMapping")]
    pub user_mapping: HashMap<Uuid, String>
}

pub async fn get_valorant_match_user_accessible_vod_handler(data: web::Path<GenericMatchPathInput>, app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    // We need to get a list of VOD information for each player in the match filtered by
    // the users that the input user has access to. We assume that the ACL on the user_id is
    // appropriate and taken care of externally.
    let vods = app.find_accessible_vods_in_match_for_user(&data.match_uuid, session.user.id).await?;

    // Note that for each VOD we also need to figure out the mapping from user uuid to puuid.
    let user_uuids: Vec<Uuid> = vods.iter()
        .filter(|x| { x.user_uuid.is_some() })
        .map(|x| { x.user_uuid.unwrap().clone() })
        .collect();

    Ok(HttpResponse::Ok().json(ValorantUserAccessibleVodOutput{
        vods,
        user_mapping: HashMap::from_iter(app.get_puuids_in_valorant_match_from_user_uuids(&data.match_uuid, &user_uuids).await?)
    }))
}