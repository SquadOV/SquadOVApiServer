pub mod auto;
pub mod rabbitmq;

use crate::{
    SquadOvError,
    SquadSharingSettings,
    SquadWowSharingSettings,
    SquadOvGames,
    SquadOvWowRelease,
};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use sqlx::{Executor, Postgres, postgres::PgPool};
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Serialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct MatchVideoSharePermissions {
    pub id: i64,
    pub can_share: bool,
    pub can_clip: bool,
}

#[derive(Clone, Serialize,Deserialize)]
#[serde(rename_all="camelCase")]
pub struct MatchVideoShareConnection {
    pub can_share: bool,
    pub can_clip: bool,
    pub id: i64,
    pub match_uuid: Option<Uuid>,
    pub video_uuid: Option<Uuid>,
    pub dest_user_id: Option<i64>,
    pub dest_squad_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct LinkShareData {
    pub is_link_shared: bool,
    pub share_url: Option<String>,
}

#[derive(Debug)]
pub struct ShareEdge {
    id: i64,
    /*
    match_uuid: Option<Uuid>,
    video_uuid: Option<Uuid>,
    source_user_id: i64,
    dest_user_id: Option<i64>,
    dest_squad_id: Option<i64>,
    */
    can_share: bool,
    can_clip: bool,
    parent_connection_id: Option<i64>,
    /*
    share_depth: i32,
    */
    is_terminal: bool,
}

fn trace_edge_permission(start: &ShareEdge, graph: &HashMap<i64, ShareEdge>, current: MatchVideoSharePermissions) -> MatchVideoSharePermissions {
    let start_perm = MatchVideoSharePermissions {
        id: -1,
        can_share: start.can_share,
        can_clip: start.can_clip,
    };

    let merge_parent = if let Some(parent_id) = &start.parent_connection_id {
        if let Some(parent_edge) = graph.get(parent_id) {
            trace_edge_permission(parent_edge, graph, start_perm)
        } else {
            start_perm
        }
    } else {
        start_perm
    };

    MatchVideoSharePermissions {
        id: -1,
        can_share: merge_parent.can_share && current.can_share,
        can_clip: merge_parent.can_clip && current.can_clip,
    }
}

pub async fn get_match_vod_share_connections_for_user<'a, T>(ex: T, match_uuid: Option<&Uuid>, video_uuid: Option<&Uuid>, user_id: i64) -> Result<Vec<MatchVideoShareConnection>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            MatchVideoShareConnection,
            r#"
            SELECT id AS "id!", match_uuid, video_uuid, can_share AS "can_share!", can_clip AS "can_clip!", dest_user_id, dest_squad_id
            FROM squadov.share_match_vod_connections
            WHERE source_user_id = $1
                AND ($2::UUID IS NULL OR match_uuid = $2)
                AND ($3::UUID IS NULL OR video_uuid = $3)
            "#,
            user_id,
            match_uuid,
            video_uuid,
        )
            .fetch_all(ex)
            .await?
    )
}

pub async fn get_match_vod_share_permissions_for_user<'a, T>(ex: T, match_uuid: Option<&Uuid>, video_uuid: Option<&Uuid>, user_id: i64) -> Result<MatchVideoSharePermissions, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    // We're looking for whether or not the user has can_share or can_clip access to the
    // current match/video in question. There are potentially multiple sources of this
    // share access and multiple paths that get to the user in question. To make it simple,
    // as long as there is a single path to the specified user with a permission enabled,
    // then we assume that the user has that permission.
    let edges = sqlx::query_as!(
        ShareEdge,
        r#"
        WITH RECURSIVE access_cte AS (
            SELECT vau.*, TRUE AS "is_terminal"
            FROM squadov.view_share_connections_access_users AS vau
            WHERE ($2::UUID IS NULL OR vau.match_uuid = $2)
                AND ($3::UUID IS NULL OR vau.video_uuid = $3)
                AND vau.user_id = $1
            UNION
            SELECT vau.*, FALSE AS "is_terminal"
            FROM squadov.view_share_connections_access_users AS vau
            INNER JOIN access_cte AS ac
                ON ac.parent_connection_id = vau.id
        )
        SELECT mvc.id AS "id!", mvc.can_share AS "can_share!", mvc.can_clip AS "can_clip!", mvc.parent_connection_id, at.is_terminal AS "is_terminal!"
        FROM access_cte AS at
        INNER JOIN squadov.share_match_vod_connections AS mvc
            ON mvc.id = at.id
        "#,
        user_id,
        match_uuid,
        video_uuid,
    )
        .fetch_all(ex)
        .await?;

    // First separate out the terminal edges from the non terminal edges.
    // The "terminal" edges are those the edges that connect immediately to the user in question.
    let mut terminal_edges: Vec<ShareEdge> = vec![];
    let mut other_edges: HashMap<i64, ShareEdge> = HashMap::new();
    edges.into_iter().for_each(|x| {
        if x.is_terminal {
            terminal_edges.push(x);
        } else {
            other_edges.insert(x.id, x);
        }
    });

    let mut permission = MatchVideoSharePermissions{
        id: -1,
        can_share: false,
        can_clip: false,
    };

    for te in terminal_edges {
        let p = trace_edge_permission(&te, &other_edges, MatchVideoSharePermissions{
            id: -1,
            can_share: true,
            can_clip: true,
        });

        permission.id = te.id;
        permission.can_share |= p.can_share;
        permission.can_clip |= p.can_clip;
    }
    
    Ok(permission)
}

pub async fn create_new_share_connection<'a, T>(ex: T, conn: &MatchVideoShareConnection, source_user_id: i64, parent_connection_id: Option<i64>) -> Result<MatchVideoShareConnection, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            MatchVideoShareConnection,
            r#"
            INSERT INTO squadov.share_match_vod_connections (
                match_uuid,
                video_uuid,
                source_user_id,
                dest_user_id,
                dest_squad_id,
                can_share,
                can_clip,
                parent_connection_id,
                share_depth
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                $6,
                $7,
                $8,
                COALESCE(
                    (
                        SELECT share_depth
                        FROM squadov.share_match_vod_connections
                        WHERE id = $8
                    ),
                    -1
                ) + 1
            )
            RETURNING
                id AS "id!",
                match_uuid,
                video_uuid,
                dest_user_id,
                dest_squad_id,
                can_share AS "can_share!",
                can_clip AS "can_clip!"
            "#,
            conn.match_uuid,
            conn.video_uuid,
            source_user_id,
            conn.dest_user_id,
            conn.dest_squad_id,
            conn.can_share,
            conn.can_clip,
            parent_connection_id,
        )
            .fetch_one(ex)
            .await?
    )
}

pub async fn delete_share_connection<'a, T>(ex: T, conn_id: i64, user_id: i64) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        DELETE FROM squadov.share_match_vod_connections
        WHERE id = $1 AND source_user_id = $2
        ",
        conn_id,
        user_id,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn edit_share_connection<'a, T>(ex: T, conn_id: i64, user_id: i64, can_share: bool, can_clip: bool) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.share_match_vod_connections
        SET can_share = $3,
            can_clip = $4
        WHERE id = $1 AND source_user_id = $2
        ",
        conn_id,
        user_id,
        can_share,
        can_clip,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn get_squad_sharing_settings(ex: &PgPool, squad_id: i64) -> Result<SquadSharingSettings, SquadOvError> {
    Ok(
        SquadSharingSettings{
            disabled_games: sqlx::query!(
                "
                SELECT disabled_game
                FROM squadov.squad_sharing_games_filter
                WHERE squad_id = $1
                ",
                squad_id,
            )
                .fetch_all(ex)
                .await?
                .into_iter()
                .map(|x| { Ok(SquadOvGames::try_from(x.disabled_game)?) })
                .collect::<Result<Vec<SquadOvGames>, SquadOvError>>()?,
            wow: sqlx::query!(
                "
                SELECT
                    disable_encounters,
                    disable_dungeons,
                    disable_keystones,
                    disable_arenas,
                    disable_bgs,
                    disabled_releases
                FROM squadov.squad_sharing_wow_filters
                WHERE squad_id = $1
                ",
                squad_id
            )
                .fetch_optional(ex)
                .await?
                .map_or::<Result<SquadWowSharingSettings, SquadOvError>, _>(Ok(SquadWowSharingSettings::default()), |x| {
                    Ok(SquadWowSharingSettings{
                        disabled_releases: x.disabled_releases.into_iter().map(|y| {
                            Ok(SquadOvWowRelease::try_from(y)?)
                        }).collect::<Result<Vec<SquadOvWowRelease>, SquadOvError>>()?,
                        disable_encounters: x.disable_encounters,
                        disable_dungeons: x.disable_dungeons,
                        disable_keystones: x.disable_keystones,
                        disable_arenas: x.disable_arenas,
                        disable_bgs: x.disable_bgs,
                    })
                })?
        }
    )
}