use uuid::Uuid;
use sqlx::{Executor, Postgres};
use crate::{
    SquadOvError,
    games::SquadOvGames,
    riot::games::{
        LolPlayerMatchSummary,
        TftPlayerMatchSummary,
        ValorantPlayerMatchSummary,
    },
    aimlab::AimlabTask,
    vod::{
        VodManifest,
        VodTag,
    },
    wow::{
        WoWEncounter,
        WoWChallenge,
        WoWArena,
        WowInstance,
    },
    csgo::summary::CsgoPlayerMatchSummary,
    elastic::vod::ESVodDocument,
    subscriptions::SquadOvSubTiers,
    vod,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::convert::TryFrom;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct MatchPlayerPair {
    pub match_uuid: Uuid,
    pub player_uuid: Uuid,
}

pub async fn get_match_favorites<'a, T>(ex: T, match_uuid: &Uuid) -> Result<Vec<(i64, String)>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT DISTINCT user_id, reason
            FROM squadov.user_favorite_matches
            WHERE match_uuid = $1
            ",
            match_uuid
        )
            .fetch_all(ex)
            .await?
            .into_iter()
            .map(|x| {
                (x.user_id, x.reason)
            })
            .collect()
    )
}

pub async fn create_new_match<'a, T>(ex: T, game: SquadOvGames) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    let uuid = Uuid::new_v4();
    sqlx::query!(
        "
        INSERT INTO squadov.matches (uuid, game)
        VALUES ($1, $2)
        ",
        &uuid,
        game as i32,
    )
        .execute(ex)
        .await?;

    Ok(uuid)
}

pub async fn get_game_for_match<'a, T>(ex: T, match_uuid: &Uuid) -> Result<SquadOvGames, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT game
            FROM squadov.matches
            WHERE uuid = $1
            ",
            match_uuid
        )
            .fetch_one(ex)
            .await?
            .game
            .map(|x| {
                SquadOvGames::try_from(x).unwrap_or(SquadOvGames::Unknown)
            })
            .unwrap_or(SquadOvGames::Unknown)
    )
}

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct RecentMatchPov {
    // VOD DATA
    pub vod: VodManifest,
    pub tm: DateTime<Utc>,
    pub username: String,
    pub user_id: i64,
    pub favorite_reason: Option<String>,
    pub is_watchlist: bool,
    pub is_local: bool,
    pub tags: Vec<VodTag>,
    pub access_token: Option<String>,
    pub tier: SquadOvSubTiers,
    // GAME DATA
    pub aimlab_task: Option<AimlabTask>,
    pub lol_match: Option<LolPlayerMatchSummary>,
    pub tft_match: Option<TftPlayerMatchSummary>,
    pub valorant_match: Option<ValorantPlayerMatchSummary>,
    pub wow_challenge: Option<WoWChallenge>,
    pub wow_encounter: Option<WoWEncounter>,
    pub wow_arena: Option<WoWArena>,
    pub wow_instance: Option<WowInstance>,
    pub csgo_match: Option<CsgoPlayerMatchSummary>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct RecentMatch {
    pub match_uuid: Uuid,
    pub game: SquadOvGames,
    pub povs: Vec<RecentMatchPov>,
}

pub async fn add_match_to_collection<'a, T>(ex: T, match_uuid: &Uuid, collection_uuid: &Uuid) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.match_to_match_collection (
            collection_uuid,
            match_uuid,
            match_order
        )
        SELECT $1, $2, COALESCE(
            (SELECT MAX(match_order)
            FROM squadov.match_to_match_collection
            WHERE collection_uuid = $1
            GROUP BY collection_uuid)
        , 0) + 1
        ON CONFLICT DO NOTHING
        ",
        collection_uuid,
        match_uuid,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn create_new_match_collection<'a, T>(ex: T) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            INSERT INTO squadov.match_collections (uuid)
            VALUES ( gen_random_uuid() )
            RETURNING uuid
            ",
        )
            .fetch_one(ex)
            .await?
            .uuid
    )
}

pub async fn get_match_collection_for_match<'a, T>(ex: T, match_uuid: &Uuid) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT collection_uuid
            FROM squadov.match_to_match_collection
            WHERE match_uuid = $1
            ",
            match_uuid,
        )
            .fetch_one(ex)
            .await?
            .collection_uuid
    )
}

pub async fn is_user_in_match<'a, T>(ex: T, user_id: i64, match_uuid: &Uuid, game: SquadOvGames) -> Result<bool, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        match game {
            SquadOvGames::AimLab => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.aimlab_tasks
                        WHERE user_id = $1 AND match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::Hearthstone => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.hearthstone_match_view
                        WHERE user_id = $1 AND match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::LeagueOfLegends => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.lol_match_participants AS lmp
                        INNER JOIN squadov.lol_match_participant_identities AS lmpi
                            ON lmpi.match_uuid = lmp.match_uuid
                                AND lmpi.participant_id = lmp.participant_id
                        INNER JOIN squadov.riot_accounts AS ra
                            ON ra.summoner_id = lmpi.summoner_id
                        INNER JOIN squadov.riot_account_links AS ral
                            ON ral.puuid = ra.puuid
                        WHERE ral.user_id = $1 AND lmp.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::TeamfightTactics => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.tft_match_participants AS tmp
                        INNER JOIN squadov.riot_account_links AS ral
                            ON ral.puuid = tmp.puuid
                        WHERE ral.user_id = $1 AND tmp.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::Valorant => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.valorant_match_players AS vmp
                        INNER JOIN squadov.riot_account_links AS ral
                            ON ral.puuid = vmp.puuid
                        WHERE ral.user_id = $1 AND vmp.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::WorldOfWarcraft => 
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.wow_match_view AS wmv
                        WHERE wmv.user_id = $1 AND wmv.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::Csgo =>
                sqlx::query!(
                    r#"
                    SELECT EXISTS (
                        SELECT 1
                        FROM squadov.csgo_match_views AS cmv
                        WHERE cmv.user_id = $1 AND cmv.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
            SquadOvGames::Unknown => false,
        }   
    )
}

pub fn vod_document_to_match_pov_for_user(doc: ESVodDocument, user_id: i64,  machine_id: &str) -> RecentMatchPov {
    let fav = doc.find_favorite_reason(user_id);
    let watchlist = doc.is_on_user_watchlist(user_id);
    RecentMatchPov {
        vod: doc.manifest.clone(),
        tm: doc.vod.end_time.unwrap_or(Utc::now()),
        username: doc.owner.username,
        user_id: doc.owner.user_id,
        favorite_reason: fav,
        is_watchlist: watchlist,
        is_local: doc.storage_copies_exact.map(|copies| {
            copies.iter().any(|x| { x.spec == machine_id })
        }).unwrap_or(false),
        tags: vod::condense_raw_vod_tags(doc.tags, user_id),
        access_token: None,
        aimlab_task: doc.data.aimlab.map(|x| { x.task }),
        lol_match: doc.data.lol.map(|x| { x.summary }).flatten(),
        tft_match: doc.data.tft.map(|x| { x.summary }).flatten(),
        valorant_match: doc.data.valorant.map(|x| { x.summary }).flatten(),
        wow_challenge: doc.data.wow.as_ref().map(|x| { x.challenge.clone() }).flatten(),
        wow_encounter: doc.data.wow.as_ref().map(|x| { x.encounter.clone() }).flatten(),
        wow_arena: doc.data.wow.as_ref().map(|x| { x.arena.clone() }).flatten(),
        wow_instance: doc.data.wow.as_ref().map(|x| { x.instance.clone() }).flatten(),
        csgo_match: doc.data.csgo.map(|x| { x.pov }),
        tier: SquadOvSubTiers::Basic,
    }
}

pub fn vod_documents_to_recent_matches(documents: Vec<ESVodDocument>, user_id: i64, machine_id: &str) -> Vec<RecentMatch> {
    let mut matches: HashMap<Uuid, RecentMatch> = HashMap::new();
    for d in documents {
        if let Some(match_uuid) = d.data.match_uuid {
            if !matches.contains_key(&match_uuid) {
                matches.insert(match_uuid.clone(), RecentMatch{
                    match_uuid: match_uuid.clone(),
                    game: d.data.game,
                    povs: vec![]
                });
            }

            let parent_match = matches.get_mut(&match_uuid).unwrap();
            let new_pov = vod_document_to_match_pov_for_user(d, user_id, machine_id);
            parent_match.povs.push(new_pov);
        }
    }

    let mut matches = matches.into_values().collect::<Vec<_>>();
    matches.sort_by(|a, b| {
        b.povs.first().unwrap().tm.partial_cmp(&a.povs.first().unwrap().tm).unwrap()
    });
    matches
}