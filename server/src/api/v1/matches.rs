mod create;

pub use create::*;
use uuid::Uuid;
use crate::api;
use crate::api::auth::SquadOVSession;
use actix_web::{web, HttpResponse, HttpRequest};
use squadov_common::{
    SquadOvError,
    SquadOvGames,
    SquadOvWowRelease,
    games,
    matches::{RecentMatch, RecentMatchPov, MatchPlayerPair},
    aimlab::AimlabTask,
    riot::{
        db as riot_db,
        games::{
            LolPlayerMatchSummary,
            TftPlayerMatchSummary,
            ValorantPlayerMatchSummary,
        },
        ValorantMatchFilters,
    },
    wow::{
        WoWEncounter,
        WoWChallenge,
        WoWArena,
        WowInstance,
    },
    access::{
        AccessTokenRequest,
        AccessToken,
    },
    encrypt::{
        AESEncryptRequest,
        squadov_encrypt,
        squadov_decrypt,
    },
    stats::StatPermission,
    csgo::{
        db as csgo_db,
        summary::{
            CsgoPlayerMatchSummary,
        },
    },
    vod::{
        VodMetadata,
        VodTrack,
        VodManifest,
    },
    share,
    share::{
        LinkShareData,
    },
    vod::{
        self,
        VodTag,
        RawVodTag,
    },
};
use std::sync::Arc;
use chrono::{DateTime, Utc, TimeZone, Duration};
use std::collections::{HashMap};
use serde::{Serialize, Deserialize};
use serde_qs::actix::QsQuery;
use crate::api::v1::{
    FavoriteResponse,
    UserProfilePath,
    wow::WowListQuery,
};
use std::convert::TryFrom;

pub struct Match {
    pub uuid : Uuid
}

pub struct MatchCollection {
    pub uuid: Uuid
}

#[derive(Deserialize,Debug)]
pub struct GenericMatchPathInput {
    pub match_uuid: Uuid
}

#[derive(Debug)]
pub struct RawRecentMatchPovData {
    video_uuid: Uuid,
    user_uuid: Uuid,
    is_local: bool,
    tm: DateTime<Utc>,
    username: String,
    user_id: i64,
    favorite_reason: Option<String>,
    is_watchlist: bool,
    tags: Vec<VodTag>,
}

#[derive(Debug)]
pub struct RawRecentMatchData {
    match_uuid: Uuid,
    game: SquadOvGames,
    povs: Vec<RawRecentMatchPovData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct GenericWowQuery {
    pub encounters: WowListQuery,
    pub keystones: WowListQuery,
    pub arenas: WowListQuery,
    pub instances: WowListQuery,
}

impl Default for GenericWowQuery {
    fn default() -> Self {
        Self {
            encounters: WowListQuery::default(),
            keystones: WowListQuery::default(),
            arenas: WowListQuery::default(),
            instances: WowListQuery::default(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct RecentMatchGameQuery {
    pub wow: GenericWowQuery,
    pub valorant: ValorantMatchFilters,
}

impl Default for RecentMatchGameQuery {
    fn default() -> Self {
        Self {
            wow: GenericWowQuery::default(),
            valorant: ValorantMatchFilters::default(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct RecentMatchQuery {
    pub games: Option<Vec<SquadOvGames>>,
    pub wow_releases: Option<Vec<SquadOvWowRelease>>,
    pub tags: Option<Vec<String>>,
    pub squads: Option<Vec<i64>>,
    pub users: Option<Vec<i64>>,
    pub time_start: Option<i64>,
    pub time_end: Option<i64>,
    pub only_favorite: bool,
    pub only_watchlist: bool,
    pub vods: Option<Vec<Uuid>>,
    pub filters: RecentMatchGameQuery,
}

impl RecentMatchQuery {
    pub fn get_wow_release_db_filter(&self) -> Vec<String> {
        if let Some(games) = self.games.as_ref() {
            if games.contains(&SquadOvGames::WorldOfWarcraft) {
                self.wow_releases.as_ref().unwrap_or(&vec![]).iter().map(|x| {
                    String::from(games::wow_release_to_db_build_expression(*x))
                }).collect::<Vec<String>>()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    }
}

impl Default for RecentMatchQuery {
    fn default() -> Self {
        Self {
            games: None,
            wow_releases: None,
            tags: None,
            squads: None,
            users: None,
            time_start: None,
            time_end: None,
            only_favorite: false,
            only_watchlist: false,
            vods: None,
            filters: RecentMatchGameQuery::default(),
        }
    }
}


fn filter_recent_match_data_by_game(data: &[RawRecentMatchData], game: SquadOvGames) -> Vec<&RawRecentMatchData> {
    data
        .iter()
        .filter(|x| {
            x.game == game
        })
        .collect()
}

fn recent_match_data_uuids(data: &[&RawRecentMatchData]) -> Vec<Uuid> {
    data.iter().map(|x| { x.match_uuid.clone() }).collect()
}

fn recent_match_data_uuid_pairs(data: &[&RawRecentMatchData]) -> Vec<MatchPlayerPair> {
    let mut ret: Vec<MatchPlayerPair> = vec![];
    for d in data {
        for p in &d.povs {
            ret.push(MatchPlayerPair{
                match_uuid: d.match_uuid.clone(),
                player_uuid: p.user_uuid.clone()
            })
        }
    }
    ret
}

#[derive(Debug)]
pub struct RecentMatchHandle {
    pub match_uuid: Uuid,
    pub user_uuids: Vec<Uuid>,
}

impl api::ApiApplication {

    pub async fn get_game_for_match(&self, match_uuid: &Uuid) -> Result<SquadOvGames, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT game
                FROM squadov.matches
                WHERE uuid = $1
                ",
                match_uuid
            )
                .fetch_one(&*self.pool)
                .await?
                .game
                .map(|x| {
                    SquadOvGames::try_from(x).unwrap_or(SquadOvGames::Unknown)
                })
                .unwrap_or(SquadOvGames::Unknown)
        )
    }

    pub async fn get_recent_base_matches(&self, handles: &[RecentMatchHandle], user_id: i64) -> Result<Vec<RawRecentMatchData>, SquadOvError> {
        let mut match_uuids: Vec<Uuid> = vec![];
        let mut user_uuids: Vec<Uuid> = vec![];

        for h in handles {
            for u in &h.user_uuids {
                match_uuids.push(h.match_uuid.clone());
                user_uuids.push(u.clone());
            }
        }

        let raw_data = sqlx::query!(
            r#"
            SELECT DISTINCT
                v.video_uuid AS "video_uuid!",
                v.match_uuid AS "match_uuid!",
                v.user_uuid AS "user_uuid!",
                v.end_time AS "tm!",
                v.is_local AS "is_local!",
                ou.username AS "username!",
                ou.id AS "user_id!",
                ufm.reason AS "favorite_reason?",
                uwv.video_uuid IS NOT NULL AS "is_watchlist!",
                m.game AS "game!",
                COALESCE(JSONB_AGG(vvt.*) FILTER(WHERE vvt.video_uuid IS NOT NULL), '[]'::JSONB)  AS "tags!"
            FROM UNNEST($1::UUID[], $2::UUID[]) AS inp(match_uuid, user_uuid)
            INNER JOIN squadov.users AS ou
                ON ou.uuid = inp.user_uuid
            INNER JOIN squadov.matches AS m
                ON inp.match_uuid = m.uuid
            INNER JOIN squadov.vods AS v
                ON v.user_uuid = ou.uuid
                    AND v.match_uuid = m.uuid
            LEFT JOIN squadov.user_favorite_matches AS ufm
                ON ufm.match_uuid = m.uuid
                    AND ufm.user_id = $3
            LEFT JOIN squadov.user_watchlist_vods AS uwv
                ON uwv.video_uuid = v.video_uuid
                    AND uwv.user_id = $3
            LEFT JOIN squadov.view_vod_tags AS vvt
                ON vvt.video_uuid = v.video_uuid
            WHERE v.is_clip = FALSE
            GROUP BY v.video_uuid, v.match_uuid, v.user_uuid, v.end_time, v.is_local, ou.username, ou.id, ufm.reason, uwv.video_uuid, m.game
            "#,
            &match_uuids,
            &user_uuids,
            user_id,
        )
            .fetch_all(&*self.pool)
            .await?;

        // At this point we have data for each individual VOD we want to pull.
        // We now want to condense this into per-match data with each VOD behind a separate
        // POV.
        let mut match_map: HashMap<Uuid, RawRecentMatchData> = HashMap::new();
        for d in raw_data {
            if !match_map.contains_key(&d.match_uuid) {
                match_map.insert(d.match_uuid.clone(), RawRecentMatchData{
                    match_uuid: d.match_uuid.clone(),
                    game: SquadOvGames::try_from(d.game)?,
                    povs: vec![],
                });
            }

            let match_data = match_map.get_mut(&d.match_uuid).unwrap();
            match_data.povs.push(RawRecentMatchPovData{
                video_uuid: d.video_uuid,
                user_uuid: d.user_uuid,
                is_local: d.is_local,
                tm: d.tm,
                username: d.username,
                user_id: d.user_id,
                favorite_reason: d.favorite_reason,
                is_watchlist: d.is_watchlist,
                tags: vod::condense_raw_vod_tags(serde_json::from_value::<Vec<RawVodTag>>(d.tags)?, user_id)?,
            });
        }

        let mut ret = match_map
            .into_iter()
            .map(|(_k, v)| v)
            .filter(|v| !v.povs.is_empty())
            .collect::<Vec<RawRecentMatchData>>();

        // We need to sort each match by it's match time. We don't store this on a per-match basis
        // but on a per-POV basis. We can assume the POVs have around the same time so just use the first POV.
        ret.sort_by(|a, b| {
            b.povs[0].tm.partial_cmp(&a.povs[0].tm).unwrap()
        });
        Ok(ret)
    }

    async fn get_recent_base_matches_for_user(&self, user_id: i64, start: i64, end: i64, filter: &RecentMatchQuery, needs_profile: bool) -> Result<Vec<RawRecentMatchData>, SquadOvError> {
        let handles: Vec<RecentMatchHandle> = sqlx::query_as!(
            RecentMatchHandle,
            r#"
            SELECT sub.match_uuid AS "match_uuid!", sub.user_uuids AS "user_uuids!"
            FROM (
                SELECT DISTINCT v.match_uuid AS "match_uuid", ARRAY_AGG(v.user_uuid) AS "user_uuids", MAX(v.end_time) AS "end_time"
                FROM squadov.users AS u
                CROSS JOIN LATERAL (
                    SELECT v.*
                    FROM squadov.vods AS v
                    INNER JOIN squadov.users AS uu
                        ON v.user_uuid = uu.uuid
                    LEFT JOIN squadov.share_match_vod_connections AS svc
                        ON svc.video_uuid = v.video_uuid
                    WHERE uu.id = $1
                        AND v.match_uuid IS NOT NULL
                        AND v.user_uuid IS NOT NULL
                        AND v.start_time IS NOT NULL
                        AND v.end_time IS NOT NULL
                        AND COALESCE(v.end_time >= $7, TRUE)
                        AND COALESCE(v.end_time <= $8, TRUE)
                        AND v.is_clip = FALSE
                        AND (CARDINALITY($11::UUID[]) = 0 OR v.video_uuid = ANY($11))
                        AND (CARDINALITY($5::BIGINT[]) = 0 OR svc.dest_squad_id = ANY($5))
                    UNION
                    SELECT v.*
                    FROM squadov.view_share_connections_access_users AS vi
                    INNER JOIN squadov.vods AS v
                        ON v.video_uuid = vi.video_uuid
                    INNER JOIN squadov.share_match_vod_connections AS mvc
                        ON mvc.id = vi.id
                    WHERE vi.user_id = $1
                        AND (CARDINALITY($5::BIGINT[]) = 0 OR mvc.dest_squad_id = ANY($5))
                        AND v.match_uuid IS NOT NULL
                        AND v.user_uuid IS NOT NULL
                        AND v.start_time IS NOT NULL
                        AND v.end_time IS NOT NULL
                        AND COALESCE(v.end_time >= $7, TRUE)
                        AND COALESCE(v.end_time <= $8, TRUE)
                        AND v.is_clip = FALSE
                        AND (CARDINALITY($11::UUID[]) = 0 OR v.video_uuid = ANY($11))
                ) AS v
                INNER JOIN squadov.users AS vu
                    ON vu.uuid = v.user_uuid
                INNER JOIN squadov.matches AS m
                    ON m.uuid = v.match_uuid
                LEFT JOIN squadov.user_favorite_matches AS ufm
                    ON ufm.match_uuid = m.uuid
                        AND ufm.user_id = $1
                LEFT JOIN squadov.user_watchlist_vods AS uwv
                    ON uwv.video_uuid = v.video_uuid
                        AND uwv.user_id = $1
                LEFT JOIN squadov.user_profile_vods AS upv
                    ON upv.video_uuid = v.video_uuid
                        AND upv.user_id = u.id
                LEFT JOIN squadov.wow_match_view AS wmv
                    ON wmv.match_uuid = v.match_uuid
                        AND wmv.user_id = vu.id
                LEFT JOIN squadov.wow_encounter_view AS wev
                    ON wev.view_id = wmv.id
                LEFT JOIN squadov.wow_challenge_view AS wcv
                    ON wcv.view_id = wmv.id
                LEFT JOIN squadov.wow_arena_view AS wav
                    ON wav.view_id = wmv.id
                LEFT JOIN squadov.wow_instance_view AS wiv
                    ON wiv.view_id = wmv.id
                LEFT JOIN squadov.valorant_matches AS vm
                    ON vm.match_uuid = m.uuid
                LEFT JOIN squadov.view_vod_tags AS vvt
                    ON v.video_uuid = vvt.video_uuid
                LEFT JOIN squadov.valorant_match_computed_data AS mcd
                    ON mcd.match_uuid = vm.match_uuid
                LEFT JOIN squadov.valorant_match_pov_computed_data AS pcd
                    ON pcd.match_uuid = vm.match_uuid
                        AND pcd.user_id = vu.id
                WHERE u.id = $1
                    AND (CARDINALITY($4::INTEGER[]) = 0 OR m.game = ANY($4))
                    AND (CARDINALITY($6::BIGINT[]) = 0 OR vu.id = ANY($6))
                    AND (NOT $9::BOOLEAN OR ufm.match_uuid IS NOT NULL)
                    AND (NOT $10::BOOLEAN OR uwv.video_uuid IS NOT NULL)
                    AND (NOT $12::BOOLEAN OR upv.video_uuid IS NOT NULL)
                    AND (CARDINALITY($13::VARCHAR[]) = 0 OR wmv.build_version LIKE ANY ($13))
                    AND (wmv.id IS NULL OR wmv.build_version NOT LIKE '9.%' OR (
                        wmv.build_version LIKE '9.%'
                            AND ((
                                    wev.view_id IS NOT NULL
                                        AND (CARDINALITY($14::INTEGER[]) = 0 OR wev.instance_id = ANY($14))
                                        AND (CARDINALITY($15::INTEGER[]) = 0 OR wev.encounter_id = ANY($15))
                                        AND ($16::BOOLEAN IS NULL OR wev.success = $16)
                                        AND (CARDINALITY($17::INTEGER[]) = 0 OR wev.difficulty = ANY($17))
                                        AND (CARDINALITY($18::INTEGER[]) = 0 OR wmv.player_spec = ANY($18))
                                        AND (COALESCE(wmv.t0_specs, '') ~ $19 OR COALESCE(wmv.t1_specs, '') ~ $19)
                                        AND $34
                                )
                                OR (
                                    wcv.view_id IS NOT NULL
                                        AND (CARDINALITY($20::INTEGER[]) = 0 OR wcv.instance_id = ANY($20)) 
                                        AND ($21::BOOLEAN IS NULL OR wcv.success = $21)
                                        AND ($22::INTEGER IS NULL OR wcv.keystone_level >= $22)
                                        AND ($23::INTEGER IS NULL OR wcv.keystone_level <= $23)
                                        AND (CARDINALITY($24::INTEGER[]) = 0 OR wmv.player_spec = ANY($24))
                                        AND (COALESCE(wmv.t0_specs, '') ~ $25 OR COALESCE(wmv.t1_specs, '') ~ $25)
                                        AND $35
                                )
                                OR (
                                    wav.view_id IS NOT NULL
                                        AND (CARDINALITY($26::INTEGER[]) = 0 OR wav.instance_id = ANY($26))
                                        AND (CARDINALITY($27::VARCHAR[]) = 0 OR wav.arena_type = ANY($27))
                                        AND ($28::BOOLEAN IS NULL OR ((wav.winning_team_id = wmv.player_team) = $28))
                                        AND (CARDINALITY($29::INTEGER[]) = 0 OR wmv.player_spec = ANY($29))
                                        AND ($30::INTEGER IS NULL OR wmv.player_rating >= $30)
                                        AND ($31::INTEGER IS NULL OR wmv.player_rating <= $31)
                                        AND (
                                            (COALESCE(wmv.t0_specs, '') ~ $32 AND COALESCE(wmv.t1_specs, '') ~ $33)
                                            OR
                                            (COALESCE(wmv.t0_specs, '') ~ $33 AND COALESCE(wmv.t1_specs, '') ~ $32)
                                        )
                                        AND $36
                                )
                                OR (
                                    wiv.view_id IS NOT NULL
                                )
                            )
                    ))
                    AND (wiv.view_id IS NULL OR (
                        $38
                            AND (CARDINALITY($39::INTEGER[]) = 0 OR wiv.instance_type = ANY($39))
                            AND (CARDINALITY($40::INTEGER[]) = 0 OR wiv.instance_id = ANY($40))
                    ))
                    AND (
                        vm.match_uuid IS NULL OR (
                            (NOT $41::BOOLEAN OR vm.is_ranked)
                                AND (CARDINALITY($42::VARCHAR[]) = 0 OR vm.map_id = ANY($42))
                                AND (CARDINALITY($43::VARCHAR[]) = 0 OR vm.game_mode = ANY($43))
                                AND (CARDINALITY($44::VARCHAR[]) = 0 OR pcd.pov_agent = ANY($44))
                                AND (NOT $45::BOOLEAN OR pcd.winner)
                                AND ($46::INTEGER IS NULL OR pcd.rank >= $46)
                                AND ($47::INTEGER IS NULL OR pcd.rank <= $47)
                                AND (CARDINALITY($48::INTEGER[]) = 0 OR pcd.key_events && $48)
                                AND (
                                    (mcd.t0_agents IS NULL AND mcd.t1_agents IS NULL)
                                    OR
                                    (mcd.t0_agents ~ $49 AND mcd.t1_agents ~ $50)
                                    OR
                                    (mcd.t0_agents ~ $50 AND mcd.t1_agents ~ $49)
                                )
                        )
                    )
                GROUP BY v.match_uuid
                HAVING CARDINALITY($37::VARCHAR[]) = 0 OR ARRAY_AGG(vvt.tag) @> $37::VARCHAR[]
            ) AS sub
            ORDER BY sub.end_time DESC
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
            }).unwrap_or(Utc::now()),
            filter.only_favorite,
            filter.only_watchlist,
            &filter.vods.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<Uuid>>(),
            needs_profile,
            &filter.get_wow_release_db_filter(),
            // Wow retail encounter filters
            &filter.filters.wow.encounters.raids.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.encounters.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.encounters.is_winner,
            &filter.filters.wow.encounters.encounter_difficulties.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.encounters.build_friendly_composition_filter()?,
            // Wow retail keystone filters
            &filter.filters.wow.keystones.dungeons.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.keystones.is_winner,
            filter.filters.wow.keystones.keystone_low,
            filter.filters.wow.keystones.keystone_high,
            &filter.filters.wow.keystones.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.keystones.build_friendly_composition_filter()?,
            // Wow retail arena filters
            &filter.filters.wow.arenas.arenas.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filter.filters.wow.arenas.brackets.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            filter.filters.wow.arenas.is_winner,
            &filter.filters.wow.arenas.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filter.filters.wow.arenas.rating_low,
            filter.filters.wow.arenas.rating_high,
            &filter.filters.wow.arenas.build_friendly_composition_filter()?,
            &filter.filters.wow.arenas.build_enemy_composition_filter()?,
            // Wow game mode filter
            &filter.filters.wow.encounters.enabled,
            &filter.filters.wow.keystones.enabled,
            &filter.filters.wow.arenas.enabled,
            // TAGS - pog
            &filter.tags.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone().to_lowercase() }).collect::<Vec<String>>(),
            // Wow instance filters
            &filter.filters.wow.instances.enabled,
            &filter.filters.wow.instances.instance_types.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x as i32 }).collect::<Vec<i32>>(),
            &filter.filters.wow.instances.all_instance_ids(),
            // Valorant
            filter.filters.valorant.is_ranked,
            &filter.filters.valorant.maps.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            &filter.filters.valorant.modes.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            &filter.filters.valorant.agent_povs.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone().to_lowercase() }).collect::<Vec<String>>(),
            filter.filters.valorant.is_winner.unwrap_or(false),
            filter.filters.valorant.rank_low,
            filter.filters.valorant.rank_high,
            &filter.filters.valorant.pov_events.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x as i32 }).collect::<Vec<i32>>(),
            &filter.filters.valorant.build_friendly_composition_filter()?,
            &filter.filters.valorant.build_enemy_composition_filter()?,
        )
            .fetch_all(&*self.heavy_pool)
            .await?;

        Ok(self.get_recent_base_matches(&handles, user_id).await?)
    }

    async fn is_match_favorite_by_user(&self, match_uuid: &Uuid, user_id: i64) -> Result<Option<String>, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                SELECT reason
                FROM squadov.user_favorite_matches
                WHERE match_uuid = $1
                    AND user_id = $2

                "#,
                match_uuid,
                user_id,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| { x.reason })
        )
    }

    async fn add_match_favorite_for_user(&self, match_uuid: &Uuid, user_id: i64, reason: &str) -> Result<(), SquadOvError> {
        sqlx::query!(
            r#"
            INSERT INTO squadov.user_favorite_matches (
                match_uuid,
                user_id,
                reason
            )
            VALUES (
                $1,
                $2,
                $3
            )
            ON CONFLICT DO NOTHING
            "#,
            match_uuid,
            user_id,
            reason,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    async fn remove_match_favorite_for_user(&self, match_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError> {
        sqlx::query!(
            r#"
            DELETE FROM squadov.user_favorite_matches
            WHERE match_uuid = $1 AND user_id = $2
            "#,
            match_uuid,
            user_id,
        )
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_recent_matches_from_uuids(&self, raw_base_matches: Vec<RawRecentMatchData>) -> Result<Vec<RecentMatch>, SquadOvError> {
        // First grab all the relevant VOD manifests using all the unique VOD UUID's.
        let mut all_vod_uuids: Vec<Uuid> = vec![];
        for m in &raw_base_matches {
            for pov in &m.povs {
                all_vod_uuids.push(pov.video_uuid.clone());
            }
        }
        let mut vod_manifests = self.get_vod(&all_vod_uuids).await?;
        
        let aimlab_tasks = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::AimLab);

            if !recent.is_empty() {
                self.list_aimlab_matches_for_uuids(&recent_match_data_uuids(&recent)).await?.into_iter().map(|x| { (x.match_uuid.clone(), x)}).collect::<HashMap<Uuid, AimlabTask>>()
            } else {
                HashMap::new()
            }
        };
        let mut lol_matches = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::LeagueOfLegends);
            if !recent.is_empty() {
                riot_db::list_lol_match_summaries_for_uuids(&*self.pool, &recent_match_data_uuid_pairs(&recent)).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), LolPlayerMatchSummary>>()
            } else {
                HashMap::new()
            }
        };
        // TFT, Valorant, and WoW is different because the match summary is player dependent.
        let mut wow_encounters = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::WorldOfWarcraft);
            if !recent.is_empty() {
                self.list_wow_encounter_for_uuids(&recent_match_data_uuid_pairs(&recent)).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWEncounter>>()
            } else {
                HashMap::new()
            }
        };
        let mut wow_challenges = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::WorldOfWarcraft);
            if !recent.is_empty() {
                self.list_wow_challenges_for_uuids(&recent_match_data_uuid_pairs(&recent)).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWChallenge>>()
            } else {
                HashMap::new()
            }
        };
        let mut wow_arenas = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::WorldOfWarcraft);
            if !recent.is_empty() {
                self.list_wow_arenas_for_uuids(&recent_match_data_uuid_pairs(&recent)).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WoWArena>>()
            } else {
                HashMap::new()
            }
        };
        let mut wow_instances = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::WorldOfWarcraft);
            if !recent.is_empty() {
                self.list_wow_instances_for_uuids(&recent_match_data_uuid_pairs(&recent)).await?.into_iter().map(|x| { ((x.match_uuid.clone(), x.user_uuid.clone()), x)}).collect::<HashMap<(Uuid, Uuid), WowInstance>>()
            } else {
                HashMap::new()
            }
        };
        let mut tft_matches = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::TeamfightTactics);
            if !recent.is_empty() {
                riot_db::list_tft_match_summaries_for_uuids(&*self.pool, &recent_match_data_uuid_pairs(&recent))
                    .await?
                    .into_iter()
                    .map(|x| {
                        ((x.match_uuid.clone(), x.user_uuid.clone()), x)
                    })
                    .collect::<HashMap<(Uuid, Uuid), TftPlayerMatchSummary>>()
            } else {
                HashMap::new()
            }
        };
        let mut valorant_matches = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::Valorant);
            if !recent.is_empty() {
                riot_db::list_valorant_match_summaries_for_uuids(&*self.pool, &recent_match_data_uuid_pairs(&recent))
                    .await?
                    .into_iter()
                    .map(|x| {
                        ((x.match_uuid.clone(), x.user_uuid.clone()), x)
                    })
                    .collect::<HashMap<(Uuid, Uuid), ValorantPlayerMatchSummary>>()
            } else {
                HashMap::new()
            }
        };
        let mut csgo_matches = {
            let recent = filter_recent_match_data_by_game(&raw_base_matches, SquadOvGames::Csgo);
            if !recent.is_empty() {
                csgo_db::list_csgo_match_summaries_for_uuids(&*self.pool, &recent_match_data_uuid_pairs(&recent))
                    .await?
                    .into_iter()
                    .map(|x| {
                        ((x.match_uuid.clone(), x.user_uuid.clone()), x)
                    })
                    .collect::<HashMap<(Uuid, Uuid), CsgoPlayerMatchSummary>>()
            } else {
                HashMap::new()
            }
        };

        Ok(
            raw_base_matches.into_iter().map(|x| {
                let match_uuid = x.match_uuid.clone();
                Ok(RecentMatch {
                    match_uuid: x.match_uuid,
                    game: x.game,
                    povs: x.povs.into_iter().map(|y| {
                        let key_pair = (match_uuid.clone(), y.user_uuid.clone());
                        let aimlab_task = aimlab_tasks.get(&match_uuid);
                        let lol_match = lol_matches.remove(&key_pair);
                        let tft_match = tft_matches.remove(&key_pair);
                        let valorant_match = valorant_matches.remove(&key_pair);
                        let wow_encounter = wow_encounters.remove(&key_pair);
                        let wow_challenge = wow_challenges.remove(&key_pair);
                        let wow_arena = wow_arenas.remove(&key_pair);
                        let wow_instance = wow_instances.remove(&key_pair);
                        let csgo_match = csgo_matches.remove(&key_pair);
                    
                        Ok(
                            RecentMatchPov {
                                // Need to give a dummy manifest for locally recorded VODs.
                                vod: vod_manifests.remove(&y.video_uuid).unwrap_or(VodManifest{
                                    video_tracks: vec![
                                        VodTrack{
                                            metadata: VodMetadata{
                                                video_uuid: y.video_uuid.clone(),
                                                ..VodMetadata::default()
                                            },
                                            segments: vec![],
                                            preview: None,
                                        }
                                    ]
                                }),
                                tm: y.tm,
                                username: y.username,
                                user_id: y.user_id,
                                favorite_reason: y.favorite_reason,
                                is_watchlist: y.is_watchlist,
                                is_local: y.is_local,
                                tags: y.tags,
                                access_token: None,
                                aimlab_task: aimlab_task.cloned(),
                                lol_match,
                                tft_match,
                                valorant_match,
                                wow_challenge,
                                wow_encounter,
                                wow_arena,
                                wow_instance,
                                csgo_match,
                            }
                        )
                    }).collect::<Result<Vec<RecentMatchPov>, SquadOvError>>()?,
                })
            }).collect::<Result<Vec<RecentMatch>, SquadOvError>>()?
        )
    }

    fn generate_access_token_for_recent_match(&self, match_uuid: &Uuid, game: SquadOvGames, user_id: i64, video_uuid: &Uuid) -> Result<String, SquadOvError> {
        let mut paths: Vec<String> = vec![
            format!("/v1/vod/{}", video_uuid),
            format!("/v1/vod/match/{}/user/id/{}", match_uuid, user_id),
        ];

        match game {
            SquadOvGames::AimLab => {
                paths.append(&mut vec![
                    format!("v1/aimlab/user/{}/match/{}/task", user_id, match_uuid),
                ]);
            },
            SquadOvGames::Csgo => {
                paths.append(&mut vec![
                    format!("v1/csgo/user/{}/match/{}", user_id, match_uuid),
                    format!("v1/csgo/match/{}/vods", match_uuid),
                ]);
            },
            SquadOvGames::Hearthstone => {
                paths.append(&mut vec![
                    format!("v1/hearthstone/user/{}/match/{}", user_id, match_uuid),
                    format!("v1/hearthstone/match/{}/vods",match_uuid),
                ]);
            },
            SquadOvGames::LeagueOfLegends => {
                paths.append(&mut vec![
                    format!("v1/lol/match/{}", match_uuid),
                ]);
            },
            SquadOvGames::TeamfightTactics => {
                paths.append(&mut vec![
                    format!("v1/tft/match/{}", match_uuid),
                ]);
            },
            SquadOvGames::Valorant => {
                paths.append(&mut vec![
                    format!("v1/valorant/match/{}", match_uuid),
                ]);
            },
            SquadOvGames::WorldOfWarcraft => {
                paths.append(&mut vec![
                    format!("v1/wow/users/{}/match/{}", user_id, match_uuid),
                    format!("v1/wow/match/{}/users/{}", match_uuid, user_id),
                    format!("v1/wow/match/{}/vods", match_uuid),
                    String::from("v1/wow/characters/armory"),
                ]);
            },
            _ => (),
        }

        Ok(
            AccessToken{
                // Ideally we'd refresh this somehow instead of just granting access for such a large chunk of time.
                expires: Some(Utc::now() + Duration::hours(6)),
                methods: Some(vec![String::from("GET")]),
                paths: Some(paths),
                user_id: Some(user_id),
            }.encrypt(&self.config.squadov.access_key)?
        )
    }
}

pub async fn get_vod_recent_match_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest, path: web::Path<super::GenericVodPathInput>) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let raw_base_matches = app.get_recent_base_matches_for_user(session.user.id, 0, 1, &RecentMatchQuery{
        vods: Some(vec![path.video_uuid.clone()]),
        ..RecentMatchQuery::default()
    }, false).await?;
    let matches = app.get_recent_matches_from_uuids(raw_base_matches).await?;

    if matches.is_empty() {
        Err(SquadOvError::NotFound)
    } else {
        Ok(HttpResponse::Ok().json(&matches[0]))
    }
}

async fn get_recent_matches_for_user(user_id: i64, app : web::Data<Arc<api::ApiApplication>>, req: &HttpRequest, query: QsQuery<api::PaginationParameters>, mut filter: web::Json<RecentMatchQuery>, needs_access_tokens: bool) -> Result<HttpResponse, SquadOvError> {
    if needs_access_tokens {
        filter.users = Some(vec![user_id]);
    }

    let raw_base_matches = app.get_recent_base_matches_for_user(user_id, query.start, query.end, &filter, needs_access_tokens).await?;
    let got_total = raw_base_matches.len() as i64;
    let mut matches = app.get_recent_matches_from_uuids(raw_base_matches).await?;

    // In this case each match needs an access token that can be used to access data for that particular match (VODs, matches, etc.).
    if needs_access_tokens {
        for m in &mut matches {
            for p in &mut m.povs {
                p.access_token = Some(app.generate_access_token_for_recent_match(&m.match_uuid, m.game, p.user_id, &p.vod.video_tracks[0].metadata.video_uuid)?);
            }
        }
    }

    let expected_total = query.end - query.start;  
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(&matches, req, &query, expected_total == got_total)?)) 
}

pub async fn get_recent_matches_for_me_handler(app : web::Data<Arc<api::ApiApplication>>, req: HttpRequest, query: QsQuery<api::PaginationParameters>, filter: web::Json<RecentMatchQuery>) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    get_recent_matches_for_user(session.user.id, app, &req, query, filter, false).await
}

pub async fn get_profile_matches_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<UserProfilePath>, req: HttpRequest, query: QsQuery<api::PaginationParameters>, filter: web::Json<RecentMatchQuery>) -> Result<HttpResponse, SquadOvError> {
    get_recent_matches_for_user(path.profile_id, app, &req, query, filter, true).await
}

#[derive(Deserialize,Debug)]
#[serde(rename_all="camelCase")]
pub struct MatchShareSignatureData {
    full_path: String,
    game: SquadOvGames,
    graphql_stats: Option<Vec<StatPermission>>,
    user_id: i64,
}

#[derive(Deserialize,Debug)]
#[serde(rename_all="camelCase")]
pub struct MatchSharePermQuery {
    game: SquadOvGames,
}

pub async fn get_match_share_connections_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    Ok(
        HttpResponse::Ok().json(
            share::get_match_vod_share_connections_for_user(&*app.pool, Some(&path.match_uuid), None, session.user.id).await?
        )
    )
}

pub async fn delete_match_share_link_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    squadov_common::access::delete_encrypted_access_token_for_match_user(&*app.pool, &path.match_uuid, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn get_match_share_link_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    let token = squadov_common::access::find_encrypted_access_token_for_match_user(&*app.pool, &path.match_uuid, session.user.id).await?;

    Ok(
        HttpResponse::Ok().json(
            LinkShareData{
                is_link_shared: token.is_some(),
                share_url: if let Some(token) = token {
                    Some(format!(
                        "{}/share/{}",
                        &app.config.cors.domain,
                        &squadov_common::access::get_share_url_identifier_for_id(&*app.pool, &token).await?,
                    ))
                } else {
                    None
                },
            }
        )
    )
}

pub async fn create_match_share_signature_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, data: web::Json<MatchShareSignatureData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    if !squadov_common::matches::is_user_in_match(&*app.pool, session.user.id, &path.match_uuid, data.game).await? {
        let permissions = share::get_match_vod_share_permissions_for_user(&*app.pool, Some(&path.match_uuid), None, session.user.id).await?;
        if !permissions.can_share {
            return Err(SquadOvError::Unauthorized);
        }
    }
    
    // If the user already shared this match, reuse that token so we don't fill up our databases with a bunch of useless tokens.
    let mut token = squadov_common::access::find_encrypted_access_token_for_match_user(&*app.pool, &path.match_uuid, session.user.id).await?;

    // We want to share all the VODs we have access to.
    let vods = app.find_accessible_vods_in_match_for_user(&path.match_uuid.clone(), session.user.id).await?;
    
    let mut video_uuids: Vec<Uuid> = vec![];
    for v in vods {
        let can_share = {
            if let Some(user_uuid) = &v.user_uuid {
                user_uuid == &session.user.uuid
            } else {
                false
            }
        } || {
            let permissions = share::get_match_vod_share_permissions_for_user(&*app.pool, None, Some(&v.video_uuid), session.user.id).await?;
            permissions.can_share
        };
        // Sanity check to make sure user has permission to share the VOD itself - otherwise we don't include in the list of VODs the user has access to
        // and don't bother trying to make it public.
        if can_share {
            video_uuids.push(v.video_uuid);
        }
    }

    if token.is_none() {
        // Now that we've verified all these things we can go ahead and return to the user a fully fleshed out
        // URL that can be shared. We enable this by generating an encrypted access token that can be used to imitate 
        // access as this session's user to ONLY this current match UUID (along with an optional VOD UUID if one exists).
        let access_request = AccessTokenRequest{
            full_path: data.full_path.clone(),
            user_uuid: session.user.uuid.clone(),
            meta_user_id: Some(data.user_id),
            match_uuid: Some(path.match_uuid.clone()),
            video_uuid: video_uuids.first().cloned(),
            bulk_video_uuids: video_uuids.clone(),
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
        let token_id = squadov_common::access::store_encrypted_access_token_for_match_user(&mut tx, &path.match_uuid, &video_uuids, session.user.id, data.user_id, &encryption_token).await?;
        squadov_common::access::generate_friendly_share_token(&mut tx, &token_id).await?;
        tx.commit().await?;

        token = Some(token_id);
    }

    // Make the VOD public - we need to keep track of its public setting in our database as well as configure the backend
    // to enable it to be served publically.
    for uuid in &video_uuids {
        app.make_vod_public(&uuid).await?;
    }

    let token = token.ok_or(SquadOvError::InternalError(String::from("Failed to obtain/generate share token.")))?;

    // It could be neat to store some sort of access token ID in our database and allow users to track how
    // many times it was used and be able to revoke it and stuff but I don't think the gains are worth it at
    // the moment. I'd rather have a more distributed version where we toss a URL out there and just let it be
    // valid.
    Ok(
        HttpResponse::Ok().json(
            LinkShareData{
                is_link_shared: true,
                share_url: Some(
                    format!(
                        "{}/share/{}",
                        &app.config.cors.domain,
                        &squadov_common::access::get_share_url_identifier_for_id(&*app.pool, &token).await?,
                    )
                ),
            }
        )
    )
}

#[derive(Deserialize,Debug)]
#[serde(rename_all="camelCase")]
pub struct MatchFavoriteData {
    reason: String,
}

pub async fn favorite_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, data: web::Json<MatchFavoriteData>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.add_match_favorite_for_user(&path.match_uuid, session.user.id, &data.reason).await?;
    Ok(HttpResponse::NoContent().finish())
}

pub async fn check_favorite_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    let reason = app.is_match_favorite_by_user(&path.match_uuid, session.user.id).await?;
    Ok(HttpResponse::Ok().json(
        FavoriteResponse{
            favorite: reason.is_some(),
            reason,
        }
    ))
}

pub async fn remove_favorite_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };

    app.remove_match_favorite_for_user(&path.match_uuid, session.user.id).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[derive(Deserialize,Debug)]
pub struct ExchangeShareTokenPath {
    access_token_id: String
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ShareTokenResponse {
    full_path: String,
    key: String,
    uid: i64,
}

pub async fn exchange_access_token_id_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<ExchangeShareTokenPath>) -> Result<HttpResponse, SquadOvError> {
    let token = squadov_common::access::find_encrypted_access_token_from_flexible_id(&*app.pool, &path.access_token_id).await?;
    let key = token.to_string();
    let req = squadov_decrypt(token, &app.config.squadov.share_key)?;

    let access = serde_json::from_slice::<AccessTokenRequest>(&req.data)?;
    Ok(HttpResponse::Ok().json(&ShareTokenResponse{
        full_path: access.full_path,
        key,
        uid: app.users.get_stored_user_from_uuid(&access.user_uuid, &*app.pool).await?.ok_or(SquadOvError::NotFound)?.id,
    }))
}