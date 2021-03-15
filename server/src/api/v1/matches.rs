mod create;

pub use create::*;
use uuid::Uuid;
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    matches::{RecentMatch, BaseRecentMatch, MatchPlayerPair},
    aimlab::AimlabTask,
    riot::db,
    riot::games::{
        LolPlayerMatchSummary,
        TftPlayerMatchSummary,
        ValorantPlayerMatchSummary,
    },
    wow::{
        WoWEncounter,
        WoWChallenge,
        WoWArena,
    },
    access::AccessTokenRequest,
    encrypt::{
        AESEncryptRequest,
        squadov_encrypt,
        squadov_decrypt,
    },
    stats::StatPermission,
};
use std::sync::Arc;
use chrono::{DateTime, Utc, TimeZone};
use std::collections::{HashSet, HashMap};
use serde::{Serialize, Deserialize};
use url::Url;
use serde_qs::actix::QsQuery;

pub struct Match {
    pub uuid : Uuid
}

pub struct MatchCollection {
    pub uuid: Uuid
}

#[derive(Deserialize,Debug)]
pub struct GenericMatchPathInput {
    match_uuid: Uuid
}

struct RawRecentMatchData {
    video_uuid: Uuid,
    match_uuid: Uuid,
    user_uuid: Uuid,
    tm: DateTime<Utc>,
    username: String,
    user_id: i64
}


#[derive(Deserialize)]
pub struct RecentMatchQuery {
    pub games: Option<Vec<SquadOvGames>>,
    pub squads: Option<Vec<i64>>,
    pub users: Option<Vec<i64>>,
    #[serde(rename="timeStart")]
    pub time_start: Option<i64>,
    #[serde(rename="timeEnd")]
    pub time_end: Option<i64>,
}

impl api::ApiApplication {

    async fn get_recent_base_matches_for_user(&self, user_id: i64, start: i64, end: i64, filter: &RecentMatchQuery) -> Result<Vec<RawRecentMatchData>, SquadOvError> {
        Ok(
            sqlx::query_as!(
                RawRecentMatchData,
                r#"
                SELECT DISTINCT
                    v.video_uuid AS "video_uuid!",
                    v.match_uuid AS "match_uuid!",
                    v.user_uuid AS "user_uuid!",
                    v.end_time AS "tm!",
                    ou.username AS "username!",
                    ou.id AS "user_id!"
                FROM squadov.users AS u
                LEFT JOIN squadov.squad_role_assignments AS sra
                    ON sra.user_id = u.id
                LEFT JOIN squadov.squad_role_assignments AS ora
                    ON ora.squad_id = sra.squad_id
                INNER JOIN squadov.users AS ou
                    ON ou.id = ora.user_id
                        OR ou.id = u.id
                INNER JOIN squadov.vods AS v
                    ON v.user_uuid = ou.uuid
                INNER JOIN squadov.matches AS m
                    ON v.match_uuid = m.uuid
                WHERE u.id = $1
                    AND v.match_uuid IS NOT NULL
                    AND v.user_uuid IS NOT NULL
                    AND v.start_time IS NOT NULL
                    AND v.end_time IS NOT NULL
                    AND (CARDINALITY($4::INTEGER[]) = 0 OR m.game = ANY($4))
                    AND (CARDINALITY($5::BIGINT[]) = 0 OR sra.squad_id = ANY($5))
                    AND (CARDINALITY($6::BIGINT[]) = 0 OR ou.id = ANY($6))
                    AND COALESCE(v.end_time >= $7, TRUE)
                    AND COALESCE(v.end_time <= $8, TRUE)
                ORDER BY v.end_time DESC
                LIMIT $2 OFFSET $3
                "#,
                user_id,
                end - start,
                start,
                &filter.games.as_ref().unwrap_or(&vec![]).iter().map(|x| {
                    *x as i32
                }).collect::<Vec<i32>>(),
                &filter.squads.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i64>>(),
                &filter.users.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i64>>(),
                filter.time_start.map(|x| {
                    Utc.timestamp_millis(x)
                }),
                filter.time_end.map(|x| {
                    Utc.timestamp_millis(x)
                }),
            )
                .fetch_all(&*self.pool)
                .await?
        )
    }

}

pub async fn get_recent_matches_for_me_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest, query: QsQuery<api::PaginationParameters>, filter: QsQuery<RecentMatchQuery>) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let raw_base_matches = app.get_recent_base_matches_for_user(session.user.id, query.start, query.end, &filter).await?;

    // First grab all the relevant VOD manifests using all the unique VOD UUID's.
    let mut vod_manifests = app.get_vod(&raw_base_matches.iter().map(|x| { x.video_uuid.clone() }).collect::<Vec<Uuid>>()).await?;

    // Now we need to grab the match summary for each of the matches. Note that this will span a multitude of games
    // so we need to bulk grab as much as possible to reduce the # of trips to the DB.
    let match_uuids = raw_base_matches.iter().map(|x| { x.match_uuid.clone() }).collect::<Vec<Uuid>>();
    let match_player_pairs = raw_base_matches.iter().map(|x| {
        MatchPlayerPair{
            match_uuid: x.match_uuid.clone(),
            player_uuid: x.user_uuid.clone(),
        }
    }).collect::<Vec<MatchPlayerPair>>();
    
    let aimlab_tasks = app.list_aimlab_matches_for_uuids(&match_uuids).await?.into_iter().map(|x| { (x.match_uuid.clone(), x)}).collect::<HashMap<Uuid, AimlabTask>>();
    let lol_matches = db::list_lol_match_summaries_for_uuids(&*app.pool, &match_uuids).await?.into_iter().map(|x| { (x.match_uuid.clone(), x)}).collect::<HashMap<Uuid, LolPlayerMatchSummary>>();
    let hearthstone_matches = app.filter_hearthstone_match_uuids(&match_uuids).await?.into_iter().collect::<HashSet<Uuid>>();
    // TFT, Valorant, and WoW is different because the match summary is player dependent.
    let mut wow_encounters = app.list_wow_encounter_for_uuids(&match_player_pairs).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWEncounter>>();
    let mut wow_challenges = app.list_wow_challenges_for_uuids(&match_player_pairs).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWChallenge>>();
    let mut wow_arenas = app.list_wow_arenas_for_uuids(&match_player_pairs).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWArena>>();
    let tft_match_uuids: HashSet<Uuid> = db::filter_tft_match_uuids(&*app.pool, &match_uuids).await?.into_iter().collect();
    let mut tft_matches = db::list_tft_match_summaries_for_uuids(&*app.pool, &match_player_pairs)
        .await?
        .into_iter()
        .map(|x| {
            ((x.match_uuid.clone(), x.user_uuid.clone()), x)
        })
        .collect::<HashMap<(Uuid, Uuid), TftPlayerMatchSummary>>();
    let mut valorant_matches = db::list_valorant_match_summaries_for_uuids(&*app.pool, &match_player_pairs)
        .await?
        .into_iter()
        .map(|x| {
            ((x.match_uuid.clone(), x.user_uuid.clone()), x)
        })
        .collect::<HashMap<(Uuid, Uuid), ValorantPlayerMatchSummary>>();
    
    let expected_total = query.end - query.start;
    let got_total = raw_base_matches.len() as i64;
    
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(raw_base_matches.into_iter().map(|x| {
        // Aim Lab, LoL, and WoW match data can be shared across multiple users hence we can't remove any
        // data from the hash maps. TFT and Valorant summary data is player specific hence why it can be removed.
        let key_pair = (x.match_uuid.clone(), x.user_uuid.clone());
        let aimlab_task = aimlab_tasks.get(&x.match_uuid);
        let lol_match = lol_matches.get(&x.match_uuid);
        let tft_match = tft_matches.remove(&key_pair);
        let valorant_match = valorant_matches.remove(&key_pair);
        let wow_encounter = wow_encounters.remove(&key_pair);
        let wow_challenge = wow_challenges.remove(&key_pair);
        let wow_arena = wow_arenas.remove(&key_pair);

        Ok(RecentMatch {
            base: BaseRecentMatch{
                match_uuid: x.match_uuid.clone(),
                tm: x.tm,
                game: if aimlab_task.is_some() {
                    SquadOvGames::AimLab
                } else if lol_match.is_some() {
                    SquadOvGames::LeagueOfLegends
                // We require an additional check for Tft match UUIDs because there's a possibility that the 
                // user didn't actually finish the match yet in which case the match UUID exists but the match
                // details don't.
                } else if tft_match.is_some() || tft_match_uuids.contains(&x.match_uuid) {
                    SquadOvGames::TeamfightTactics
                } else if valorant_match.is_some() {
                    SquadOvGames::Valorant
                } else if wow_encounter.is_some() || wow_challenge.is_some() || wow_arena.is_some() {
                    SquadOvGames::WorldOfWarcraft
                } else if hearthstone_matches.contains(&x.match_uuid) {
                    SquadOvGames::Hearthstone
                } else {
                    SquadOvGames::Unknown
                },
                vod: vod_manifests.remove(&x.video_uuid).ok_or(SquadOvError::InternalError(String::from("Failed to find expected VOD manifest.")))?,
                username: x.username,
                user_id: x.user_id,
            },
            aimlab_task: aimlab_task.cloned(),
            lol_match: lol_match.cloned(),
            tft_match,
            valorant_match,
            wow_challenge,
            wow_encounter,
            wow_arena,
        })
    }).collect::<Result<Vec<RecentMatch>, SquadOvError>>()?, &req, &query, expected_total == got_total)?)) 
}

#[derive(Deserialize,Debug)]
#[serde(rename_all="camelCase")]
pub struct MatchShareSignatureData {
    full_path: String,
    game: SquadOvGames,
    graphql_stats: Option<Vec<StatPermission>>,
}

pub async fn create_match_share_signature_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, data: web::Json<MatchShareSignatureData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    // We need to verify that the requesting user is actually a part of the match. Note that we should not
    // check for the presence of a VOD because we should keep it possible to share matches that are VOD-less.
    if !squadov_common::matches::is_user_in_match(&*app.pool, session.user.id, &path.match_uuid, data.game).await? {
        return Err(SquadOvError::Unauthorized);
    }

    // Next we need to generate the share URL for this match. This is dependent on
    // 1) The app domain
    // 2) The game of the match being requested.
    // 3) The user's POV that's being requested.
    // #1 is something the API server should know while #2 is something more along the lines of what
    // the client (user) should know. HOWEVER, we shouldn't fully trust them so we need to examine the
    // URL that they sent to do further verification. Generally the only two things we need to verify are
    // 1) The user ID (should match the session user id)
    // 2) The Riot PUUID (only in certain cases).
    let base_url = format!(
        "{}{}",
        &app.config.cors.domain,
        &data.full_path,
    );
    let parsed_url = Url::parse(&base_url)?;
    for qp in parsed_url.query_pairs() {
        if qp.0 == "userId" {
            if qp.1.parse::<i64>()? != session.user.id {
                return Err(SquadOvError::Unauthorized);
            }
        } else if qp.0 == "puuid" {
            if !db::is_riot_puuid_linked_to_user(&*app.pool, session.user.id, &qp.1).await? {
                return Err(SquadOvError::Unauthorized);
            }
        }
    }

    // If the user already shared this match, reuse that token so we don't fill up our databases with a bunch of useless tokens.
    let mut token = squadov_common::access::find_encrypted_access_token_for_match_user(&*app.pool, &path.match_uuid, session.user.id).await?;

    if token.is_none() {
        // Now that we've verified all these things we can go ahead and return to the user a fully fleshed out
        // URL that can be shared. We enable this by generating an encrypted access token that can be used to imitate 
        // access as this session's user to ONLY this current match UUID (along with an optional VOD UUID if one exists).
        let access_request = AccessTokenRequest{
            full_path: data.full_path.clone(),
            user_uuid: session.user.uuid.clone(),
            match_uuid: Some(path.match_uuid.clone()),
            video_uuid: app.find_vod_from_match_user_id(path.match_uuid.clone(), session.user.id).await?.map(|x| {
                x.video_uuid
            }),
            clip_uuid: None,
            graphql_stats: data.graphql_stats.clone(),
        };

        let encryption_request = AESEncryptRequest{
            data: serde_json::to_vec(&access_request)?,
            aad: session.user.uuid.as_bytes().to_vec(),
        };

        let encryption_token = squadov_encrypt(encryption_request, &app.config.squadov.share_key)?;

        // Store the encrypted token in our database and return to the user a URL with the unique ID and the IV.
        // This way we get a (relatively) shorter URL instead of a giant encrypted blob.
        let mut tx = app.pool.begin().await?;
        let token_id = squadov_common::access::store_encrypted_access_token_for_match_user(&mut tx, &path.match_uuid, session.user.id, &encryption_token).await?;
        tx.commit().await?;

        token = Some(token_id);
    }

    let token = token.ok_or(SquadOvError::InternalError(String::from("Failed to obtain/generate share token.")))?;

    // It could be neat to store some sort of access token ID in our database and allow users to track how
    // many times it was used and be able to revoke it and stuff but I don't think the gains are worth it at
    // the moment. I'd rather have a more distributed version where we toss a URL out there and just let it be
    // valid.
    Ok(HttpResponse::Ok().json(&format!(
        "{}/share/{}",
        &app.config.cors.domain,
        &token,
    )))
}

#[derive(Deserialize,Debug)]
pub struct ExchangeShareTokenPath {
    access_token_id: Uuid
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ShareTokenResponse {
    full_path: String,
    key: String,
    uid: i64,
}

pub async fn exchange_access_token_id_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<ExchangeShareTokenPath>) -> Result<HttpResponse, SquadOvError> {
    let token = squadov_common::access::find_encrypted_access_token_from_id(&*app.pool, &path.access_token_id).await?;
    let key = token.to_string();
    let req = squadov_decrypt(token, &app.config.squadov.share_key)?;

    let access = serde_json::from_slice::<AccessTokenRequest>(&req.data)?;
    Ok(HttpResponse::Ok().json(&ShareTokenResponse{
        full_path: access.full_path,
        key,
        uid: app.users.get_stored_user_from_uuid(&access.user_uuid, &*app.pool).await?.ok_or(SquadOvError::NotFound)?.id,
    }))
}