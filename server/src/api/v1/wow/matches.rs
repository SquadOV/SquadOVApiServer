use squadov_common::{
    SquadOvError,
    SquadOvGames,
    WoWEncounterStart,
    WoWChallengeStart,
    WoWArenaStart,
    WoWEncounterEnd,
    WoWChallengeEnd,
    WoWArenaEnd,
    WoWEncounter,
    WoWChallenge,
    WoWArena,
    WoWCombatantInfo,
    WoWCombatLogState,
    matches::MatchPlayerPair,
    generate_combatants_key,
};
use actix_web::{web, HttpResponse, HttpRequest};
use crate::api;
use crate::api::auth::SquadOVSession;
use crate::api::v1::GenericMatchPathInput;
use squadov_common::vod::VodAssociation;
use std::sync::Arc;
use uuid::Uuid;
use sqlx::{Postgres, Transaction};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use serde_qs::actix::QsQuery;

#[derive(Deserialize)]
pub struct GenericMatchCreationRequest<T> {
    pub timestamp: DateTime<Utc>,
    pub data: T,
    pub cl: WoWCombatLogState,
}

#[derive(Deserialize)]
pub struct GenericMatchFinishCreationRequest<T> {
    pub timestamp: DateTime<Utc>,
    pub data: T,
    pub combatants: Vec<WoWCombatantInfo>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct WowListQuery {
    pub has_vod: Option<bool>,
    pub encounters: Option<Vec<i32>>,
    pub raids: Option<Vec<i32>>,
    pub dungeons: Option<Vec<i32>>,
    pub arenas: Option<Vec<i32>>,
    pub brackets: Option<Vec<String>>,
    pub rating_low: Option<i32>,
    pub rating_high: Option<i32>,
    pub friendly_composition: Option<Vec<String>>,
    pub enemy_composition: Option<Vec<String>>,
    pub pov_spec: Option<Vec<i32>>,
    pub encounter_difficulties: Option<Vec<i32>>,
    pub keystone_low: Option<i32>,
    pub keystone_high: Option<i32>,
    // If not set, wins + losses. If true, only wins. If false, only losses.
    pub is_winner: Option<bool>,
    pub enabled: bool,
}

impl Default for WowListQuery {
    fn default() -> Self {
        Self {
            has_vod: None,
            encounters: None,
            raids: None,
            dungeons: None,
            arenas: None,
            brackets: None,
            rating_low: None,
            rating_high: None,
            friendly_composition: None,
            enemy_composition: None,
            pov_spec: None,
            encounter_difficulties: None,
            keystone_low: None,
            keystone_high: None,
            is_winner: None,
            enabled: true,
        }
    }
}

impl WowListQuery {
    pub fn build_friendly_composition_filter(&self) -> Result<String, SquadOvError> {
        WowListQuery::build_composition_filter(self.friendly_composition.as_ref())
    }

    pub fn build_enemy_composition_filter(&self) -> Result<String, SquadOvError> {
        WowListQuery::build_composition_filter(self.enemy_composition.as_ref())
    }

    fn build_composition_filter(f: Option<&Vec<String>>) -> Result<String, SquadOvError> {
        Ok(
            if let Some(inner) = f {
                let mut pieces: Vec<String> = vec![];
                for x in inner {
                    // Each string is going to be a JSON array of integers [1, 2, 3].
                    let json_arr: Vec<i32> = serde_json::from_str(x)?;

                    // It could be empty in which case we want to match anything.
                    if json_arr.is_empty() {
                        continue;
                    }

                    // Each JSON array needs to be converted into a regex lookahead group
                    // that looks like: (?=.*,(1|2|3),)
                    pieces.push(format!(
                        "(?=.*,({}),)",
                        json_arr.into_iter().map(|x| {
                            format!("{}", x)
                        })
                            .collect::<Vec<String>>()
                            .join("|")
                    ));
                }
                format!("^{}.*$", pieces.join(""))
            } else {
                String::from(".*")
            }
        )
    }
}

impl api::ApiApplication {
    async fn filter_valid_wow_match_player_pairs(&self, uuids: &[MatchPlayerPair]) -> Result<(Vec<Uuid>, Vec<i64>), SquadOvError> {
        let match_uuids = uuids.iter().map(|x| { x.match_uuid.clone() }).collect::<Vec<Uuid>>();
        let player_uuids = uuids.iter().map(|x| { x.player_uuid.clone() }).collect::<Vec<Uuid>>();
        
        let final_identifiers = sqlx::query!(
            r#"
            SELECT
                inp.match_uuid AS "match_uuid!",
                u.id AS "user_id!"
            FROM UNNEST($1::UUID[], $2::UUID[]) AS inp(match_uuid, player_uuid)
            INNER JOIN squadov.users AS u
                ON u.uuid = inp.player_uuid
            INNER JOIN squadov.wow_match_view AS wmv
                ON wmv.user_id = u.id
                    AND wmv.match_uuid = inp.match_uuid
            "#,
            &match_uuids,
            &player_uuids,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .map(|x| {
                (x.match_uuid, x.user_id)
            })
            .collect::<Vec<(Uuid, i64)>>();
        
        let match_uuids = final_identifiers.iter().map(|x| { x.0.clone() }).collect::<Vec<Uuid>>();
        let player_ids = final_identifiers.iter().map(|x| { x.1 }).collect::<Vec<i64>>();
        Ok((match_uuids, player_ids))
    }

    async fn list_wow_encounters_for_character(&self, character_guid: &str, user_id: i64, req_user_id: i64, start: i64, end: i64, filters: &WowListQuery) -> Result<Vec<WoWEncounter>, SquadOvError> {
        let pairs = sqlx::query!(
            r#"
            SELECT DISTINCT
                wmv.match_uuid AS "match_uuid!",
                u.uuid AS "player_uuid!",
                wmv.start_tm
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_encounter_view AS wav
                ON wav.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_combatants AS wvc
                ON wvc.character_id = wcp.character_id
            INNER JOIN squadov.users AS u
                ON u.id = wmv.user_id
            LEFT JOIN squadov.vods AS v
                ON v.match_uuid = wmv.match_uuid
                    AND v.user_uuid = u.uuid
                    AND v.is_clip = FALSE
            LEFT JOIN squadov.view_share_connections_access_users AS sau
                ON sau.match_uuid = wmv.match_uuid
                    AND sau.user_id = $8
            CROSS JOIN LATERAL (
                SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                FROM (
                    SELECT MIN(wvc.spec_id)
                    FROM squadov.wow_match_view_character_presence AS wcp
                    INNER JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    WHERE wcp.view_id = wmv.id
                    GROUP BY wcp.view_id, wcp.unit_guid
                ) sub(val)
            ) AS specs(s)
            WHERE wmv.user_id = $2
                AND wcp.unit_guid = $1
                AND wmv.match_uuid IS NOT NULL
                AND (CARDINALITY($5::INTEGER[]) = 0 OR wav.instance_id = ANY($5))
                AND (CARDINALITY($6::INTEGER[]) = 0 OR wav.encounter_id = ANY($6))
                AND (NOT $7::BOOLEAN OR v.video_uuid IS NOT NULL)
                AND ($2 = $8 OR sau.match_uuid IS NOT NULL)
                AND ($9::BOOLEAN IS NULL OR wav.success = $9)
                AND (CARDINALITY($10::INTEGER[]) = 0 OR wav.difficulty = ANY($10))
                AND (CARDINALITY($11::INTEGER[]) = 0 OR wvc.spec_id = ANY($11))
                AND specs.s ~ $12
            ORDER BY wmv.start_tm DESC, wmv.match_uuid, u.uuid
            LIMIT $3 OFFSET $4
            "#,
            character_guid,
            user_id,
            end - start,
            start,
            &filters.raids.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filters.encounters.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filters.has_vod.unwrap_or(false),
            req_user_id,
            filters.is_winner,
            &filters.encounter_difficulties.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filters.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filters.build_friendly_composition_filter()?,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .map(|x| {
                MatchPlayerPair{
                    match_uuid: x.match_uuid,
                    player_uuid: x.player_uuid,
                }
            })
            .collect::<Vec<MatchPlayerPair>>();
        Ok(self.list_wow_encounter_for_uuids(&pairs).await?)
    }

    pub async fn list_wow_encounter_for_uuids(&self, uuids: &[MatchPlayerPair]) -> Result<Vec<WoWEncounter>, SquadOvError> {
        let (match_uuids, user_ids) = self.filter_valid_wow_match_player_pairs(uuids).await?;

        Ok(
            sqlx::query_as!(
                WoWEncounter,
                r#"
                SELECT * FROM (
                    SELECT DISTINCT ON (wmv.match_uuid, u.uuid)
                        wmv.match_uuid AS "match_uuid!",
                        wmv.start_tm AS "tm!",
                        wmv.end_tm AS "finish_time", 
                        wmv.build_version AS "build!",
                        u.uuid AS "user_uuid!",
                        wa.combatants_key,
                        wav.encounter_id,
                        wav.encounter_name,
                        wav.difficulty,
                        wav.num_players,
                        wav.instance_id,
                        COALESCE(wav.success, FALSE) AS "success!"
                    FROM UNNEST($1::UUID[], $2::BIGINT[]) AS inp(match_uuid, user_id)
                    INNER JOIN squadov.wow_match_view AS wmv
                        ON wmv.match_uuid = inp.match_uuid
                            AND wmv.user_id = inp.user_id
                    INNER JOIN squadov.new_wow_encounters AS wa
                        ON wa.match_uuid = wmv.match_uuid
                    INNER JOIN squadov.wow_encounter_view AS wav
                        ON wav.view_id = wmv.id
                    INNER JOIN squadov.users AS u
                        ON u.id = wmv.user_id
                    ORDER BY wmv.match_uuid, u.uuid
                ) AS t
                ORDER BY finish_time DESC
                "#,
                &match_uuids,
                &user_ids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn list_wow_challenges_for_character(&self, character_guid: &str, user_id: i64, req_user_id: i64, start: i64, end: i64, filters: &WowListQuery) -> Result<Vec<WoWChallenge>, SquadOvError> {
        let pairs = sqlx::query!(
            r#"
            SELECT DISTINCT
                wmv.match_uuid AS "match_uuid!",
                u.uuid AS "player_uuid!",
                wmv.start_tm
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_challenge_view AS wav
                ON wav.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_combatants AS wvc
                ON wvc.character_id = wcp.character_id
            INNER JOIN squadov.users AS u
                ON u.id = wmv.user_id
            CROSS JOIN LATERAL (
                SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                FROM (
                    SELECT MIN(wvc.spec_id)
                    FROM squadov.wow_match_view_character_presence AS wcp
                    INNER JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    WHERE wcp.view_id = wmv.id
                    GROUP BY wcp.view_id, wcp.unit_guid
                ) sub(val)
            ) AS specs(s)
            LEFT JOIN squadov.vods AS v
                ON v.match_uuid = wmv.match_uuid
                    AND v.user_uuid = u.uuid
                    AND v.is_clip = FALSE
            LEFT JOIN squadov.view_share_connections_access_users AS sau
                ON sau.match_uuid = wmv.match_uuid
                    AND sau.user_id = $7
            WHERE wmv.user_id = $2
                AND wcp.unit_guid = $1
                AND wmv.match_uuid IS NOT NULL
                AND (CARDINALITY($5::INTEGER[]) = 0 OR wav.instance_id = ANY($5))
                AND (NOT $6::BOOLEAN OR v.video_uuid IS NOT NULL)
                AND ($2 = $7 OR sau.match_uuid IS NOT NULL)
                AND ($8::BOOLEAN IS NULL OR wav.success = $8)
                AND ($9::INTEGER IS NULL OR wav.keystone_level >= $9)
                AND ($10::INTEGER IS NULL OR wav.keystone_level <= $10)
                AND (CARDINALITY($11::INTEGER[]) = 0 OR wvc.spec_id = ANY($11))
                AND specs.s ~ $12
            ORDER BY wmv.start_tm DESC, wmv.match_uuid, u.uuid
            LIMIT $3 OFFSET $4
            "#,
            character_guid,
            user_id,
            end - start,
            start,
            &filters.dungeons.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filters.has_vod.unwrap_or(false),
            req_user_id,
            filters.is_winner,
            filters.keystone_low,
            filters.keystone_high,
            &filters.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            &filters.build_friendly_composition_filter()?,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .map(|x| {
                MatchPlayerPair{
                    match_uuid: x.match_uuid,
                    player_uuid: x.player_uuid,
                }
            })
            .collect::<Vec<MatchPlayerPair>>();
        Ok(self.list_wow_challenges_for_uuids(&pairs).await?)
    }

    pub async fn list_wow_challenges_for_uuids(&self, uuids: &[MatchPlayerPair]) -> Result<Vec<WoWChallenge>, SquadOvError> {
        let (match_uuids, user_ids) = self.filter_valid_wow_match_player_pairs(uuids).await?;

        Ok(
            sqlx::query_as!(
                WoWChallenge,
                r#"
                SELECT * FROM (
                    SELECT DISTINCT ON (wmv.match_uuid, u.uuid)
                        wmv.match_uuid AS "match_uuid!",
                        wmv.start_tm AS "tm!",
                        wmv.end_tm AS "finish_time", 
                        wmv.build_version AS "build!",
                        u.uuid AS "user_uuid!",
                        wa.combatants_key,
                        wav.challenge_name,
                        wav.instance_id,
                        wav.keystone_level,
                        COALESCE(wav.time_ms, 0) AS "time_ms!",
                        COALESCE(wav.success, FALSE) AS "success!"
                    FROM UNNEST($1::UUID[], $2::BIGINT[]) AS inp(match_uuid, user_id)
                    INNER JOIN squadov.wow_match_view AS wmv
                        ON wmv.match_uuid = inp.match_uuid
                            AND wmv.user_id = inp.user_id
                    INNER JOIN squadov.new_wow_challenges AS wa
                        ON wa.match_uuid = wmv.match_uuid
                    INNER JOIN squadov.wow_challenge_view AS wav
                        ON wav.view_id = wmv.id
                    INNER JOIN squadov.users AS u
                        ON u.id = wmv.user_id
                    ORDER BY wmv.match_uuid, u.uuid
                ) AS t
                ORDER BY finish_time DESC
                "#,
                &match_uuids,
                &user_ids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn list_wow_arenas_for_character(&self, character_guid: &str, user_id: i64, req_user_id: i64, start: i64, end: i64, filters: &WowListQuery) -> Result<Vec<WoWArena>, SquadOvError> {
        let pairs = sqlx::query!(
            r#"
            SELECT DISTINCT
                wmv.match_uuid AS "match_uuid!",
                u.uuid AS "player_uuid!",
                wmv.start_tm
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_arena_view AS wav
                ON wav.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_character_presence AS wcp
                ON wcp.view_id = wmv.id
            INNER JOIN squadov.wow_match_view_combatants AS mvc
                ON mvc.character_id = wcp.character_id
            CROSS JOIN LATERAL (
                SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                FROM (
                    SELECT MIN(wvc.spec_id)
                    FROM squadov.wow_match_view_character_presence AS wcp
                    INNER JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    WHERE wcp.view_id = wmv.id
                        AND wvc.team = 0
                    GROUP BY wcp.view_id, wcp.unit_guid
                ) sub(val)
            ) AS t0(s)
            CROSS JOIN LATERAL (
                SELECT ',' || STRING_AGG(val::VARCHAR, ',') || ',' AS vv
                FROM (
                    SELECT MIN(wvc.spec_id)
                    FROM squadov.wow_match_view_character_presence AS wcp
                    INNER JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    WHERE wcp.view_id = wmv.id
                        AND wvc.team = 1
                    GROUP BY wcp.view_id, wcp.unit_guid
                ) sub(val)
            ) AS t1(s)
            INNER JOIN squadov.users AS u
                ON u.id = wmv.user_id
            LEFT JOIN squadov.vods AS v
                ON v.match_uuid = wmv.match_uuid
                    AND v.user_uuid = u.uuid
                    AND v.is_clip = FALSE
            LEFT JOIN squadov.view_share_connections_access_users AS sau
                ON sau.match_uuid = wmv.match_uuid
                    AND sau.user_id = $7
            WHERE wmv.user_id = $2
                AND wcp.unit_guid = $1
                AND wmv.match_uuid IS NOT NULL
                AND (CARDINALITY($5::INTEGER[]) = 0 OR wav.instance_id = ANY($5))
                AND (NOT $6::BOOLEAN OR v.video_uuid IS NOT NULL)
                AND ($2 = $7 OR sau.match_uuid IS NOT NULL)
                AND (CARDINALITY($8::VARCHAR[]) = 0 OR wav.arena_type = ANY($8))
                AND ($9::BOOLEAN IS NULL OR ((wav.winning_team_id = mvc.team) = $9))
                AND (CARDINALITY($10::INTEGER[]) = 0 OR mvc.spec_id = ANY($10))
                AND ($11::INTEGER IS NULL OR mvc.rating >= $11)
                AND ($12::INTEGER IS NULL OR mvc.rating <= $12)
                AND (
                    (t0.s ~ $13 AND t1.s ~ $14)
                    OR
                    (t0.s ~ $14 AND t1.s ~ $13)
                )
            ORDER BY wmv.start_tm DESC, wmv.match_uuid, u.uuid
            LIMIT $3 OFFSET $4
            "#,
            character_guid,
            user_id,
            end - start,
            start,
            &filters.arenas.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filters.has_vod.unwrap_or(false),
            req_user_id,
            &filters.brackets.as_ref().unwrap_or(&vec![]).iter().map(|x| { x.clone() }).collect::<Vec<String>>(),
            filters.is_winner,
            &filters.pov_spec.as_ref().unwrap_or(&vec![]).iter().map(|x| { *x }).collect::<Vec<i32>>(),
            filters.rating_low,
            filters.rating_high,
            &filters.build_friendly_composition_filter()?,
            &filters.build_enemy_composition_filter()?,
        )
            .fetch_all(&*self.heavy_pool)
            .await?
            .into_iter()
            .map(|x| {
                MatchPlayerPair{
                    match_uuid: x.match_uuid,
                    player_uuid: x.player_uuid,
                }
            })
            .collect::<Vec<MatchPlayerPair>>();
        Ok(self.list_wow_arenas_for_uuids(&pairs).await?)
    }

    pub async fn list_wow_arenas_for_uuids(&self, uuids: &[MatchPlayerPair]) -> Result<Vec<WoWArena>, SquadOvError> {
        let (match_uuids, user_ids) = self.filter_valid_wow_match_player_pairs(uuids).await?;

        Ok(
            sqlx::query_as!(
                WoWArena,
                r#"
                SELECT * FROM (
                    SELECT DISTINCT ON (wmv.match_uuid, u.uuid)
                        wmv.match_uuid AS "match_uuid!",
                        wmv.start_tm AS "tm!",
                        wmv.end_tm AS "finish_time", 
                        wmv.build_version AS "build!",
                        wa.combatants_key,
                        wav.instance_id,
                        wav.arena_type,
                        wav.winning_team_id,
                        wav.match_duration_seconds,
                        wav.new_ratings,
                        u.uuid AS "user_uuid",
                        (
                            CASE WHEN wvc.event_id IS NOT NULL THEN wvc.team = wav.winning_team_id
                                ELSE FALSE
                            END
                        ) AS "success!"
                    FROM UNNEST($1::UUID[], $2::BIGINT[]) AS inp(match_uuid, user_id)
                    INNER JOIN squadov.wow_match_view AS wmv
                        ON wmv.match_uuid = inp.match_uuid
                            AND wmv.user_id = inp.user_id
                    INNER JOIN squadov.new_wow_arenas AS wa
                        ON wa.match_uuid = wmv.match_uuid
                    INNER JOIN squadov.wow_arena_view AS wav
                        ON wav.view_id = wmv.id
                    INNER JOIN squadov.wow_match_view_character_presence AS wcp
                        ON wcp.view_id = wmv.id
                    LEFT JOIN squadov.wow_match_view_combatants AS wvc
                        ON wvc.character_id = wcp.character_id
                    INNER JOIN squadov.wow_user_character_cache AS wucc
                        ON wucc.unit_guid = wcp.unit_guid
                            AND wucc.user_id = inp.user_id
                    INNER JOIN squadov.users AS u
                        ON u.id = wmv.user_id
                    ORDER BY wmv.match_uuid, u.uuid
                ) AS t
                ORDER BY finish_time DESC
                "#,
                &match_uuids,
                &user_ids,
            )
                .fetch_all(&*self.heavy_pool)
                .await?
        )
    }

    async fn find_wow_challenge(&self, match_uuid: &Uuid, user_uuid: &Uuid) -> Result<Option<WoWChallenge>, SquadOvError> {
        let pairs = vec![MatchPlayerPair{
            match_uuid: match_uuid.clone(),
            player_uuid: user_uuid.clone(),
        }];

        let mut challenges = self.list_wow_challenges_for_uuids(&pairs).await?;
        Ok(challenges.pop())
    }

    async fn find_wow_encounter(&self, match_uuid: &Uuid, user_uuid: &Uuid) -> Result<Option<WoWEncounter>, SquadOvError> {
        let pairs = vec![MatchPlayerPair{
            match_uuid: match_uuid.clone(),
            player_uuid: user_uuid.clone(),
        }];

        let mut encounters = self.list_wow_encounter_for_uuids(&pairs).await?;
        Ok(encounters.pop())
    }

    async fn find_wow_arena(&self, match_uuid: &Uuid, user_uuid: &Uuid) -> Result<Option<WoWArena>, SquadOvError> {
        let pairs = vec![MatchPlayerPair{
            match_uuid: match_uuid.clone(),
            player_uuid: user_uuid.clone(),
        }];

        let mut arenas = self.list_wow_arenas_for_uuids(&pairs).await?;
        Ok(arenas.pop())
    }

    pub async fn get_wow_match_view_for_user_match(&self, user_id: i64, match_uuid: &Uuid) -> Result<Option<Uuid>, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT id
                FROM squadov.wow_match_view
                WHERE user_id = $1
                    AND match_uuid = $2
                ",
                user_id,
                match_uuid
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    x.id
                })
        )
    }

    async fn create_generic_wow_match_view(&self, tx: &mut Transaction<'_, Postgres>, tm: &DateTime<Utc>, user_id: i64, cl: &WoWCombatLogState) -> Result<Uuid, SquadOvError> {
        Ok(
            sqlx::query!(
                r#"
                INSERT INTO squadov.wow_match_view (
                    id,
                    user_id,
                    start_tm,
                    combat_log_version,
                    advanced_log,
                    build_version
                )
                VALUES (
                    gen_random_uuid(),
                    $1,
                    $2,
                    $3,
                    $4,
                    $5
                )
                RETURNING id
                "#,
                user_id,
                tm,
                &cl.combat_log_version,
                cl.advanced_log,
                &cl.build_version,
            )
                .fetch_one(tx)
                .await?
                .id
        )
    }

    pub async fn create_wow_encounter_match_view(&self, tx: &mut Transaction<'_, Postgres>, tm: &DateTime<Utc>, user_id: i64, game: &WoWEncounterStart, cl: &WoWCombatLogState) -> Result<Uuid, SquadOvError> {
        let uuid = self.create_generic_wow_match_view(&mut *tx, tm, user_id, cl).await?;
        sqlx::query!(
            "
            INSERT INTO squadov.wow_encounter_view (
                view_id,
                encounter_id,
                encounter_name,
                difficulty,
                num_players,
                instance_id
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6
            )
            ",
            &uuid,
            game.encounter_id,
            &game.encounter_name,
            game.difficulty,
            game.num_players,
            game.instance_id,
        )
            .execute(&mut *tx)
            .await?;
        Ok(uuid)
    }

    pub async fn find_existing_wow_encounter_match(&self, view_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<Option<Uuid>, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT match_uuid
                FROM squadov.new_wow_encounters AS wc
                CROSS JOIN (
                    SELECT wmv.start_tm, wcv.encounter_id, wcv.difficulty, wcv.instance_id
                    FROM squadov.wow_match_view AS wmv
                    INNER JOIN squadov.wow_encounter_view AS wcv
                        ON wcv.view_id = wmv.id
                    WHERE wmv.id = $2
                ) AS wmv(start_tm, encounter_id, difficulty, instance_id)
                WHERE wc.tr && tstzrange(wmv.start_tm, $3, '[]')
                    AND wc.combatants_key = $1
                    AND wc.encounter_id = wmv.encounter_id
                    AND wc.difficulty = wmv.difficulty
                    AND wc.instance_id = wmv.instance_id
                ",
                key,
                view_uuid,
                tm,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    x.match_uuid
                })
        )
    }

    pub async fn finish_wow_encounter_match(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<(), SquadOvError> {
        // Insert into wow encounters table.
        sqlx::query!(
            "
            INSERT INTO squadov.new_wow_encounters (
                match_uuid,
                tr,
                combatants_key,
                encounter_id,
                difficulty,
                instance_id
            )
            SELECT
                $1,
                tstzrange(wmv.start_tm, $4, '[]'),
                $2,
                wev.encounter_id,
                wev.difficulty,
                wev.instance_id
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_encounter_view AS wev
                ON wev.view_id = wmv.id
            WHERE wmv.id = $3
            ",
            match_uuid,
            key,
            view_uuid,
            tm,
        )
            .execute(&mut *tx)
            .await?;

        Ok(())
    }

    pub async fn finish_wow_encounter_view(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, game: &WoWEncounterEnd) -> Result<(), SquadOvError> {
        // Modify view to link to the new match and to update the end time as well.
        sqlx::query!(
            "
            UPDATE squadov.wow_match_view
            SET end_tm = $2,
                match_uuid = $3
            WHERE id = $1
            ",
            view_uuid,
            tm,
            match_uuid,
        )
            .execute(&mut *tx)
            .await?;

        // Modify game specific view with data parameters.
        sqlx::query!(
            "
            UPDATE squadov.wow_encounter_view
            SET success = $2
            WHERE view_id = $1
            ",
            view_uuid,
            game.success,
        )
            .execute(&mut *tx)
            .await?;
        
        Ok(())
    }

    pub async fn create_wow_challenge_match_view(&self, tx: &mut Transaction<'_, Postgres>, tm: &DateTime<Utc>, user_id: i64, game: &WoWChallengeStart, cl: &WoWCombatLogState) -> Result<Uuid, SquadOvError> {
        let uuid = self.create_generic_wow_match_view(&mut *tx, tm, user_id, cl).await?;
        sqlx::query!(
            "
            INSERT INTO squadov.wow_challenge_view (
                view_id,
                challenge_name,
                instance_id,
                keystone_level
            )
            VALUES (
                $1,
                $2,
                $3,
                $4
            )
            ",
            &uuid,
            &game.challenge_name,
            game.instance_id,
            game.keystone_level,
        )
            .execute(&mut *tx)
            .await?;
        Ok(uuid)
    }

    pub async fn find_existing_wow_challenge_match(&self, view_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<Option<Uuid>, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT match_uuid
                FROM squadov.new_wow_challenges AS wc
                CROSS JOIN (
                    SELECT wmv.start_tm, wcv.instance_id, wcv.keystone_level
                    FROM squadov.wow_match_view AS wmv
                    INNER JOIN squadov.wow_challenge_view AS wcv
                        ON wcv.view_id = wmv.id
                    WHERE wmv.id = $2
                ) AS wmv(start_tm, instance_id, keystone_level)
                WHERE wc.tr && tstzrange(wmv.start_tm, $3, '[]')
                    AND wc.combatants_key = $1
                    AND wc.instance_id = wmv.instance_id
                    AND wc.keystone_level = wmv.keystone_level
                ",
                key,
                view_uuid,
                tm,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    x.match_uuid
                })
        )
    }

    pub async fn finish_wow_challenge_view(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, game: &WoWChallengeEnd) -> Result<(), SquadOvError> {
        // Modify view to link to the new match and to update the end time as well.
        sqlx::query!(
            "
            UPDATE squadov.wow_match_view
            SET end_tm = $2,
                match_uuid = $3
            WHERE id = $1
            ",
            view_uuid,
            tm,
            match_uuid,
        )
            .execute(&mut *tx)
            .await?;

        // Modify game specific view with data parameters.
        sqlx::query!(
            "
            UPDATE squadov.wow_challenge_view
            SET success = $2,
                time_ms = $3
            WHERE view_id = $1
            ",
            view_uuid,
            game.success,
            game.time_ms,
        )
            .execute(&mut *tx)
            .await?;
        
        Ok(())
    }

    pub async fn finish_wow_challenge_match(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<(), SquadOvError> {
        // Insert into wow encounters table.
        sqlx::query!(
            "
            INSERT INTO squadov.new_wow_challenges (
                match_uuid,
                tr,
                combatants_key,
                instance_id,
                keystone_level
            )
            SELECT
                $1,
                tstzrange(wmv.start_tm, $4, '[]'),
                $2,
                wcv.instance_id,
                wcv.keystone_level
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_challenge_view AS wcv
                ON wcv.view_id = wmv.id
            WHERE wmv.id = $3
            ON CONFLICT (match_uuid) DO NOTHING
            ",
            match_uuid,
            key,
            view_uuid,
            tm,
        )
            .execute(&mut *tx)
            .await?;
        Ok(())
    }

    pub async fn create_wow_arena_match_view(&self, tx: &mut Transaction<'_, Postgres>, tm: &DateTime<Utc>, user_id: i64, game: &WoWArenaStart, cl: &WoWCombatLogState) -> Result<Uuid, SquadOvError> {
        let uuid = self.create_generic_wow_match_view(&mut *tx, tm, user_id, cl).await?;
        sqlx::query!(
            "
            INSERT INTO squadov.wow_arena_view (
                view_id,
                instance_id,
                arena_type
            )
            VALUES (
                $1,
                $2,
                $3
            )
            ",
            &uuid,
            game.instance_id,
            &game.arena_type
        )
            .execute(&mut *tx)
            .await?;
        Ok(uuid)
    }

    pub async fn find_existing_wow_arena_match(&self, view_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<Option<Uuid>, SquadOvError> {
        Ok(
            sqlx::query!(
                "
                SELECT match_uuid
                FROM squadov.new_wow_arenas AS wc
                CROSS JOIN (
                    SELECT wmv.start_tm, wcv.instance_id, wcv.arena_type
                    FROM squadov.wow_match_view AS wmv
                    INNER JOIN squadov.wow_arena_view AS wcv
                        ON wcv.view_id = wmv.id
                    WHERE wmv.id = $2
                ) AS wmv(start_tm, instance_id, arena_type)
                WHERE wc.tr && tstzrange(wmv.start_tm, $3, '[]')
                    AND wc.combatants_key = $1
                    AND wc.instance_id = wmv.instance_id
                    AND wc.arena_type = wmv.arena_type
                ",
                key,
                view_uuid,
                tm,
            )
                .fetch_optional(&*self.pool)
                .await?
                .map(|x| {
                    x.match_uuid
                })
        )
    }

    pub async fn finish_wow_arena_match(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, key: &str) -> Result<(), SquadOvError> {
        // Insert into wow encounters table.
        sqlx::query!(
            "
            INSERT INTO squadov.new_wow_arenas (
                match_uuid,
                tr,
                combatants_key,
                instance_id,
                arena_type
            )
            SELECT
                $1,
                tstzrange(wmv.start_tm, $4, '[]'),
                $2,
                wav.instance_id,
                wav.arena_type
            FROM squadov.wow_match_view AS wmv
            INNER JOIN squadov.wow_arena_view AS wav
                ON wav.view_id = wmv.id
            WHERE wmv.id = $3
            ",
            match_uuid,
            key,
            view_uuid,
            tm,
        )
            .execute(&mut *tx)
            .await?;
        Ok(())
    }

    pub async fn finish_wow_arena_view(&self, tx: &mut Transaction<'_, Postgres>, view_uuid: &Uuid, match_uuid: &Uuid, tm: &DateTime<Utc>, game: &WoWArenaEnd) -> Result<(), SquadOvError> {
        // Modify view to link to the new match and to update the end time as well.
        sqlx::query!(
            "
            UPDATE squadov.wow_match_view
            SET end_tm = $2,
                match_uuid = $3
            WHERE id = $1
            ",
            view_uuid,
            tm,
            match_uuid,
        )
            .execute(&mut *tx)
            .await?;

        // Modify game specific view with data parameters.
        sqlx::query!(
            "
            UPDATE squadov.wow_arena_view
            SET winning_team_id = $2,
                match_duration_seconds = $3,
                new_ratings = $4
            WHERE view_id = $1
            ",
            view_uuid,
            game.winning_team_id,
            game.match_duration_seconds,
            &game.new_ratings,
        )
            .execute(&mut *tx)
            .await?;
        
        Ok(())
    }
}

pub async fn create_wow_encounter_match_handler(app : web::Data<Arc<api::ApiApplication>>, input_match: web::Json<GenericMatchCreationRequest<WoWEncounterStart>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let uuid = app.create_wow_encounter_match_view(&mut tx, &input_match.timestamp, session.user.id, &input_match.data, &input_match.cl).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(uuid))
}

pub async fn create_wow_challenge_match_handler(app : web::Data<Arc<api::ApiApplication>>, input_match: web::Json<GenericMatchCreationRequest<WoWChallengeStart>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let uuid = app.create_wow_challenge_match_view(&mut tx, &input_match.timestamp, session.user.id, &input_match.data, &input_match.cl).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(uuid))
}

pub async fn create_wow_arena_match_handler(app : web::Data<Arc<api::ApiApplication>>, input_match: web::Json<GenericMatchCreationRequest<WoWArenaStart>>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(x) => x,
        None => return Err(SquadOvError::BadRequest)
    };

    let mut tx = app.pool.begin().await?;
    let uuid = app.create_wow_arena_match_view(&mut tx, &input_match.timestamp, session.user.id, &input_match.data, &input_match.cl).await?;
    tx.commit().await?;
    Ok(HttpResponse::Ok().json(uuid))
}

pub async fn finish_wow_encounter_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<GenericMatchFinishCreationRequest<WoWEncounterEnd>>, path: web::Path<super::WoWViewPath>) -> Result<HttpResponse, SquadOvError> {
    let combatants_key = generate_combatants_key(&data.combatants);
    for _i in 0i32..2 {
        let mut tx = app.pool.begin().await?;
        let match_uuid = match app.find_existing_wow_encounter_match(&path.view_uuid, &data.timestamp, &combatants_key).await? {
            Some(uuid) => uuid,
            None => {
                let new_match = app.create_new_match(&mut tx, SquadOvGames::WorldOfWarcraft).await?;
                match app.finish_wow_encounter_match(&mut tx, &path.view_uuid, &new_match.uuid, &data.timestamp, &combatants_key).await {
                    Ok(_) => (),
                    Err(err) => match err {
                        SquadOvError::Duplicate => {
                            // This indicates that the match UUID is INVALID because a match with the same
                            // match ID already exists. Retry!
                            log::warn!("Caught duplicate WoW encounter...retrying!");
                            continue;
                        },
                        _ => return Err(err)
                    }
                };
                new_match.uuid
            }
        };
        app.finish_wow_encounter_view(&mut tx, &path.view_uuid, &match_uuid, &data.timestamp, &data.data).await?;
        
        tx.commit().await?;
        return Ok(HttpResponse::Ok().json(match_uuid));
    }
    Err(SquadOvError::InternalError(String::from("Too many errors in finishing WoW encounter...Retry limit reached.")))
}

pub async fn finish_wow_challenge_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<GenericMatchFinishCreationRequest<WoWChallengeEnd>>, path: web::Path<super::WoWViewPath>) -> Result<HttpResponse, SquadOvError> {
    let combatants_key = generate_combatants_key(&data.combatants);
    for _i in 0i32..2 {
        let mut tx = app.pool.begin().await?;
        let match_uuid = match app.find_existing_wow_challenge_match(&path.view_uuid, &data.timestamp, &combatants_key).await? {
            Some(uuid) => uuid,
            None => {
                let new_match = app.create_new_match(&mut tx, SquadOvGames::WorldOfWarcraft).await?;
                match app.finish_wow_challenge_match(&mut tx, &path.view_uuid, &new_match.uuid, &data.timestamp, &combatants_key).await {
                    Ok(_) => (),
                    Err(err) => match err {
                        SquadOvError::Duplicate => {
                            // This indicates that the match UUID is INVALID because a match with the same
                            // match ID already exists. Retry!
                            log::warn!("Caught duplicate WoW challenge...retrying!");
                            continue;
                        },
                        _ => return Err(err)
                    }
                };
                new_match.uuid
            }
        };
        app.finish_wow_challenge_view(&mut tx, &path.view_uuid, &match_uuid, &data.timestamp, &data.data).await?;
        
        tx.commit().await?;
        return Ok(HttpResponse::Ok().json(match_uuid));
    }
    Err(SquadOvError::InternalError(String::from("Too many errors in finishing WoW challenge...Retry limit reached.")))
}

pub async fn finish_wow_arena_handler(app : web::Data<Arc<api::ApiApplication>>, data: web::Json<GenericMatchFinishCreationRequest<WoWArenaEnd>>, path: web::Path<super::WoWViewPath>) -> Result<HttpResponse, SquadOvError> {
    let combatants_key = generate_combatants_key(&data.combatants);
    for _i in 0i32..2 {
        let mut tx = app.pool.begin().await?;
        let match_uuid = match app.find_existing_wow_arena_match(&path.view_uuid, &data.timestamp, &combatants_key).await? {
            Some(uuid) => uuid,
            None => {
                let new_match = app.create_new_match(&mut tx, SquadOvGames::WorldOfWarcraft).await?;
                match app.finish_wow_arena_match(&mut tx, &path.view_uuid, &new_match.uuid, &data.timestamp, &combatants_key).await {
                    Ok(_) => (),
                    Err(err) => match err {
                        SquadOvError::Duplicate => {
                            // This indicates that the match UUID is INVALID because a match with the same
                            // match ID already exists. Retry!
                            log::warn!("Caught duplicate WoW arena...retrying!");
                            continue;
                        },
                        _ => return Err(err)
                    }
                };
                new_match.uuid
            }
        };
        app.finish_wow_arena_view(&mut tx, &path.view_uuid, &match_uuid, &data.timestamp, &data.data).await?;
        
        tx.commit().await?;
        return Ok(HttpResponse::Ok().json(match_uuid));
    }
    Err(SquadOvError::InternalError(String::from("Too many errors in finishing WoW arena...Retry limit reached.")))
}

pub async fn list_wow_encounters_for_character_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<api::PaginationParameters>, filters: QsQuery<WowListQuery>, path: web::Path<super::WoWUserCharacterPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let query = query.into_inner();
    let encounters = app.list_wow_encounters_for_character(
        &path.character_guid,
        path.user_id,
        session.user.id,
        query.start,
        query.end,
        &filters,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = encounters.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(encounters, &req, &query, expected_total == got_total)?))
}

pub async fn list_wow_challenges_for_character_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<api::PaginationParameters>, filters: QsQuery<WowListQuery>, path: web::Path<super::WoWUserCharacterPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;
    
    let query = query.into_inner();
    let challenges = app.list_wow_challenges_for_character(
        &path.character_guid,
        path.user_id,
        session.user.id,
        query.start,
        query.end,
        &filters,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = challenges.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(challenges, &req, &query, expected_total == got_total)?))
}

pub async fn list_wow_arenas_for_character_handler(app : web::Data<Arc<api::ApiApplication>>, query: web::Query<api::PaginationParameters>, filters: QsQuery<WowListQuery>, path: web::Path<super::WoWUserCharacterPath>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = extensions.get::<SquadOVSession>().ok_or(SquadOvError::Unauthorized)?;

    let query = query.into_inner();
    let challenges = app.list_wow_arenas_for_character(
        &path.character_guid,
        path.user_id,
        session.user.id,
        query.start,
        query.end,
        &filters,
    ).await?;

    let expected_total = query.end - query.start;
    let got_total = challenges.len() as i64;
    Ok(HttpResponse::Ok().json(api::construct_hal_pagination_response(challenges, &req, &query, expected_total == got_total)?))
}

pub async fn get_wow_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<super::WoWUserMatchPath>) -> Result<HttpResponse, SquadOvError> {
    #[derive(Serialize)]
    struct Response {
        encounter: Option<WoWEncounter>,
        challenge: Option<WoWChallenge>,
        arena: Option<WoWArena>,
    }

    let uuid = app.user_id_to_uuid(path.user_id).await?;
    Ok(HttpResponse::Ok().json(Response{
        encounter: app.find_wow_encounter(&path.match_uuid, &uuid).await?,
        challenge: app.find_wow_challenge(&path.match_uuid, &uuid).await?,
        arena: app.find_wow_arena(&path.match_uuid, &uuid).await?,
    }))
}

#[derive(Serialize)]
struct WowUserAccessibleVodOutput {
    pub vods: Vec<VodAssociation>,
    #[serde(rename="userToId")]
    pub user_to_id: HashMap<Uuid, i64>
}

pub async fn list_wow_vods_for_squad_in_match_handler(app : web::Data<Arc<api::ApiApplication>>, path: web::Path<GenericMatchPathInput>, req: HttpRequest) -> Result<HttpResponse, SquadOvError> {
    let extensions = req.extensions();
    let session = match extensions.get::<SquadOVSession>() {
        Some(s) => s,
        None => return Err(SquadOvError::Unauthorized),
    };
    let vods = app.find_accessible_vods_in_match_for_user(&path.match_uuid, session.user.id).await?;

    // Note that for each VOD we also need to figure out the mapping from user uuid to puuid.
    let user_uuids: Vec<Uuid> = vods.iter()
        .filter(|x| { x.user_uuid.is_some() })
        .map(|x| { x.user_uuid.unwrap().clone() })
        .collect();

    let user_uuid_to_id = app.get_user_uuid_to_user_id_map(&user_uuids).await?;

    Ok(HttpResponse::Ok().json(WowUserAccessibleVodOutput{
        vods,
        user_to_id: user_uuid_to_id,
    }))
}