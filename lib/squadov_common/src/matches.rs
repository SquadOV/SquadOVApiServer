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
    vod::VodManifest,
    wow::{
        WoWEncounter,
        WoWChallenge,
        WoWArena,
    },
};
use chrono::{DateTime, Utc};
use serde::Serialize;

pub struct MatchPlayerPair {
    pub match_uuid: Uuid,
    pub player_uuid: Uuid,
}

pub async fn create_new_match<'a, T>(ex: T) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    let uuid = Uuid::new_v4();
    sqlx::query!(
        "
        INSERT INTO squadov.matches (uuid)
        VALUES ($1)
        ",
        &uuid,
    )
        .execute(ex)
        .await?;

    Ok(uuid)
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct BaseRecentMatch {
    pub match_uuid: Uuid,
    pub tm: DateTime<Utc>,
    pub game: SquadOvGames,
    pub vod: VodManifest,
    pub username: String,
    pub user_id: i64,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct RecentMatch {
    pub base: BaseRecentMatch,

    pub aimlab_task: Option<AimlabTask>,
    pub lol_match: Option<LolPlayerMatchSummary>,
    pub tft_match: Option<TftPlayerMatchSummary>,
    pub valorant_match: Option<ValorantPlayerMatchSummary>,
    pub wow_challenge: Option<WoWChallenge>,
    pub wow_encounter: Option<WoWEncounter>,
    pub wow_arena: Option<WoWArena>,
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
                            ON ra.account_id = lmpi.account_id
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
                        FROM squadov.wow_match_combatants AS wmc
                        INNER JOIN squadov.wow_user_character_association AS wuca
                            ON wuca.guid = wmc.combatant_guid
                        WHERE wuca.user_id = $1 AND wmc.match_uuid = $2
                    ) as "exists!"
                    "#,
                    user_id,
                    match_uuid,
                )
                    .fetch_one(ex)
                    .await?
                    .exists,
        }   
    )
}