use crate::{
    SquadOvError,
    riot::{
        games::{
            TftPlayerMatchSummary,
            TftCompanionDto,
            TftUnitDto,
            TftTraitDto,
        },
        TftMatchFilters,
    },
    matches::MatchPlayerPair,
};
use sqlx::PgPool;
use uuid::Uuid;
use std::collections::HashMap;

pub async fn filter_tft_match_uuids(ex: &PgPool, uuids: &[Uuid]) -> Result<Vec<Uuid>, SquadOvError> {
    Ok(
        sqlx::query!(
            "
            SELECT tm.match_uuid
            FROM squadov.tft_matches AS tm
            WHERE tm.match_uuid = ANY($1)
            ",
            uuids,
        )
            .fetch_all(ex)
            .await?
            .into_iter()
            .map(|x| { x.match_uuid })
            .collect()
    )
}

pub async fn list_tft_match_summaries_for_uuids(ex: &PgPool, uuids: &[MatchPlayerPair]) -> Result<Vec<TftPlayerMatchSummary>, SquadOvError> {
    let match_uuids = uuids.iter().map(|x| { x.match_uuid.clone() }).collect::<Vec<Uuid>>();
    let player_uuids = uuids.iter().map(|x| { x.player_uuid.clone() }).collect::<Vec<Uuid>>();

    let mut match_summaries: Vec<TftPlayerMatchSummary> = sqlx::query!(
        r#"
        SELECT
            tmi.match_uuid AS "match_uuid!",
            tmi.game_datetime AS "game_datetime!",
            tmi.game_length as "game_length!",
            tmi.game_variation AS "game_variation",
            tmi.game_version AS "game_version!",
            tmi.queue_id AS "queue_id!",
            tmi.tft_set_number AS "tft_set_number!",
            tmp.companion_content_id AS "companion_content_id!",
            tmp.companion_skin_id AS "companion_skin_id!",
            tmp.companion_species AS "companion_species!",
            tmp.level AS "level!",
            tmp.placement AS "placement!",
            tmp.last_round AS "last_round!",
            (vod.video_uuid IS NOT NULL) AS "has_vod!",
            inp.user_uuid AS "user_uuid!",
            ral.puuid AS "puuid!"
        FROM UNNEST($1::UUID[], $2::UUID[]) AS inp(match_uuid, user_uuid)
        INNER JOIN squadov.tft_match_info AS tmi
            ON tmi.match_uuid = inp.match_uuid
        INNER JOIN squadov.tft_match_participants AS tmp
            ON tmp.match_uuid = tmi.match_uuid
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = tmp.puuid
        INNER JOIN squadov.users AS u
            ON u.id = ral.user_id
                AND u.uuid = inp.user_uuid
        LEFT JOIN squadov.vods AS vod
            ON vod.match_uuid = tmi.match_uuid
                AND vod.user_uuid = u.uuid
                AND vod.is_clip = FALSE
        WHERE tmi.tft_set_number >= 3
        ORDER BY tmi.game_datetime DESC
        "#,
        &match_uuids,
        &player_uuids,
    )
        .fetch_all(ex)
        .await?
        .into_iter()
        .map(|x| {
            TftPlayerMatchSummary {
                match_uuid: x.match_uuid,
                game_datetime: x.game_datetime,
                game_length: x.game_length,
                game_variation: x.game_variation,
                game_version: x.game_version,
                queue_id: x.queue_id,
                tft_set_number: x.tft_set_number,
                companion: TftCompanionDto{
                    content_id: x.companion_content_id,
                    skin_id: x.companion_skin_id,
                    species: x.companion_species,
                },
                level: x.level,
                placement: x.placement,
                last_round: x.last_round,
                traits: vec![],
                units: vec![],
                has_vod: x.has_vod,
                user_uuid: x.user_uuid,
                puuid: x.puuid,
            }
        })
        .collect();

    // These need to be separate queries because SQLX doesn't really let us
    // do a ARRAY_AGG cleanly (i think?) so this is easier + type safe.
    let mut unit_map: HashMap<(Uuid, Uuid), Vec<TftUnitDto>> = HashMap::new();
    sqlx::query!(
        r#"
        SELECT
            tmpu.match_uuid AS "match_uuid!",
            tmpu.items AS "items!",
            tmpu.character_id,
            tmpu.chosen,
            tmpu.name AS "name!",
            tmpu.rarity AS "rarity!",
            tmpu.tier AS "tier!",
            u.uuid AS "user_uuid!"
        FROM UNNEST($1::UUID[], $2::UUID[]) AS inp(match_uuid, user_uuid)
        INNER JOIN squadov.users AS u
            ON u.uuid = inp.user_uuid
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.user_id = u.id
        INNER JOIN squadov.tft_match_participant_units AS tmpu
            ON tmpu.match_uuid = inp.match_uuid
                AND tmpu.puuid = ral.puuid
        "#,
        &match_uuids,
        &player_uuids,
    )
        .fetch_all(ex)
        .await?
        .into_iter()
        .for_each(|x| {
            let key = (x.match_uuid.clone(), x.user_uuid.clone());
            if !unit_map.contains_key(&key) {
                unit_map.insert(key.clone(), Vec::new());
            }
            let vec = unit_map.get_mut(&key).unwrap();
            vec.push(TftUnitDto{
                items: x.items,
                character_id: x.character_id,
                chosen: x.chosen,
                name: x.name,
                rarity: x.rarity,
                tier: x.tier,
            });
        });

    let mut trait_map: HashMap<(Uuid, Uuid), Vec<TftTraitDto>> = HashMap::new();
    sqlx::query!(
        r#"
        SELECT
            tmpt.match_uuid AS "match_uuid!",
            tmpt.name AS "name!",
            tmpt.num_units AS "num_units!",
            tmpt.style AS "style!",
            tmpt.tier_current AS "tier_current!",
            tmpt.tier_total AS "tier_total!",
            u.uuid AS "user_uuid!"
        FROM UNNEST($1::UUID[], $2::UUID[]) AS inp(match_uuid, user_uuid)
        INNER JOIN squadov.users AS u
            ON u.uuid = inp.user_uuid
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.user_id = u.id
        INNER JOIN squadov.tft_match_participant_traits AS tmpt
            ON tmpt.match_uuid = inp.match_uuid
                AND tmpt.puuid = ral.puuid
        "#,
        &match_uuids,
        &player_uuids,
    )
        .fetch_all(ex)
        .await?
        .into_iter()
        .for_each(|x| {
            let key = (x.match_uuid.clone(), x.user_uuid.clone());
            if !trait_map.contains_key(&key) {
                trait_map.insert(key.clone(), Vec::new());
            }
            let vec = trait_map.get_mut(&key).unwrap();
            vec.push(TftTraitDto{
                name: x.name,
                num_units: x.num_units,
                style: x.style,
                tier_current: x.tier_current,
                tier_total: x.tier_total,
            });
        });

    for m in &mut match_summaries {
        m.units.extend(unit_map.remove(&(m.match_uuid.clone(), m.user_uuid.clone())).unwrap_or(vec![]));
        m.traits.extend(trait_map.remove(&(m.match_uuid.clone(), m.user_uuid.clone())).unwrap_or(vec![]));
    }

    Ok(match_summaries)
}

pub async fn list_tft_match_summaries_for_puuid(ex: &PgPool, puuid: &str, user_uuid: &Uuid, req_user_id: i64, start: i64, end: i64, filters: &TftMatchFilters) -> Result<Vec<TftPlayerMatchSummary>, SquadOvError> {
    let uuids: Vec<Uuid> = sqlx::query!(
        r#"
        SELECT DISTINCT tmi.game_datetime, tmi.match_uuid
        FROM squadov.tft_match_info AS tmi
        INNER JOIN squadov.tft_match_participants AS tmp
            ON tmp.match_uuid = tmi.match_uuid
        LEFT JOIN squadov.vods AS v
            ON v.match_uuid = tmi.match_uuid
                AND v.user_uuid = $4
                AND v.is_clip = FALSE
        LEFT JOIN squadov.view_share_connections_access_users AS sau
            ON sau.match_uuid = tmi.match_uuid
                AND sau.user_id = $6
        CROSS JOIN (
            SELECT *
            FROM squadov.users
            WHERE uuid = $4
        ) AS u
        WHERE tmp.puuid = $1
            AND tmi.tft_set_number >= 3
            AND (NOT $5::BOOLEAN OR v.video_uuid IS NOT NULL)
            AND (u.id = $6 OR sau.match_uuid IS NOT NULL)
        ORDER BY tmi.game_datetime DESC, tmi.match_uuid
        LIMIT $2 OFFSET $3
        "#,
        puuid,
        end - start,
        start,
        user_uuid,
        filters.has_vod.unwrap_or(false),
        req_user_id,
    )
        .fetch_all(&*ex)
        .await?
        .into_iter()
        .map(|x| {
            x.match_uuid
        })
        .collect();

    list_tft_match_summaries_for_uuids(ex, &uuids.into_iter().map(|x| {
        MatchPlayerPair{
            match_uuid: x,
            player_uuid: user_uuid.clone(),
        }
    }).collect::<Vec<MatchPlayerPair>>()).await
}

pub async fn get_puuids_in_tft_match_from_user_uuids(ex: &PgPool, match_uuid: &Uuid, user_uuids: &[Uuid]) -> Result<Vec<(Uuid, String)>, SquadOvError> {
    Ok(
        sqlx::query!(
            "
            SELECT u.uuid, tmp.puuid
            FROM squadov.tft_match_participants AS tmp
            INNER JOIN squadov.riot_accounts AS ra
                ON ra.puuid = tmp.puuid
            INNER JOIN squadov.riot_account_links AS ral
                ON ral.puuid = ra.puuid
            INNER JOIN squadov.users AS u
                ON u.id = ral.user_id
            WHERE tmp.match_uuid = $1
                AND u.uuid = ANY($2)
            ",
            match_uuid,
            user_uuids,
        )
            .fetch_all(ex)
            .await?
            .into_iter()
            .map(|x| {
                (x.uuid, x.puuid)
            })
            .collect()
    )
}