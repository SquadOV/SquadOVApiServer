use crate::{
    SquadOvError,
    encrypt::AESEncryptToken,
    stats::StatPermission,
};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use sqlx::{Executor, Postgres};

#[derive(Serialize,Deserialize,Debug,Clone)]
#[serde(rename_all="camelCase")]
pub struct AccessTokenRequest {
    pub full_path: String,
    pub user_uuid: Uuid,
    pub match_uuid: Option<Uuid>,
    pub video_uuid: Option<Uuid>,
    pub clip_uuid: Option<Uuid>,
    pub graphql_stats: Option<Vec<StatPermission>>,
}

pub async fn delete_encrypted_access_token_for_match_user<'a, T>(ex: T, match_uuid: &Uuid, user_id: i64) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        DELETE FROM squadov.share_tokens
        WHERE match_uuid = $1 AND user_id = $2
        ",
        match_uuid,
        user_id,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn find_encrypted_access_token_for_match_user<'a, T>(ex: T, match_uuid: &Uuid, user_id: i64) -> Result<Option<Uuid>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT id
            FROM squadov.share_tokens
            WHERE match_uuid = $1 AND user_id = $2
            ",
            match_uuid,
            user_id,
        )
            .fetch_optional(ex)
            .await?
            .map(|x| {
                x.id
            })
    )
}

pub async fn find_encrypted_access_token_for_clip_user<'a, T>(ex: T, clip_uuid: &Uuid, user_id: i64) -> Result<Option<Uuid>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            SELECT id
            FROM squadov.share_tokens
            WHERE clip_uuid = $1 AND user_id = $2
            ",
            clip_uuid,
            user_id,
        )
            .fetch_optional(ex)
            .await?
            .map(|x| {
                x.id
            })
    )
}

pub async fn find_encrypted_access_token_from_id<'a, T>(ex: T, id: &Uuid) -> Result<AESEncryptToken, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            AESEncryptToken,
            r#"
            SELECT
                encrypted_token AS "data",
                iv,
                aad,
                tag
            FROM squadov.share_tokens
            WHERE id = $1
            "#,
            id,
        )
            .fetch_one(ex)
            .await?
    )
}


pub async fn store_encrypted_access_token_for_match_user<'a, T>(ex: T, match_uuid: &Uuid, user_id: i64, token: &AESEncryptToken) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            INSERT INTO squadov.share_tokens (
                id,
                match_uuid,
                user_id,
                encrypted_token,
                iv,
                aad,
                tag
            ) VALUES (
                gen_random_uuid(),
                $1,
                $2,
                $3,
                $4,
                $5,
                $6
            )
            RETURNING id
            ",
            match_uuid,
            user_id,
            token.data,
            token.iv,
            token.aad,
            token.tag
        )
            .fetch_one(ex)
            .await?
            .id
    )
}

pub async fn store_encrypted_access_token_for_clip_user<'a, T>(ex: T, clip_uuid: &Uuid, user_id: i64, token: &AESEncryptToken) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            "
            INSERT INTO squadov.share_tokens (
                id,
                clip_uuid,
                user_id,
                encrypted_token,
                iv,
                aad,
                tag
            ) VALUES (
                gen_random_uuid(),
                $1,
                $2,
                $3,
                $4,
                $5,
                $6
            )
            RETURNING id
            ",
            clip_uuid,
            user_id,
            token.data,
            token.iv,
            token.aad,
            token.tag
        )
            .fetch_one(ex)
            .await?
            .id
    )
}

pub async fn check_user_has_access_to_match_vod_from_user<'a, T>(ex: T, dest_user_id: i64, source_user_id: Option<i64>, match_uuid: Option<Uuid>, video_uuid: Option<Uuid>) -> Result<bool, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    if let Some(user_id) = source_user_id {
        if user_id == dest_user_id {
            return Ok(true);
        }
    }

    Ok(
        sqlx::query!(
            r#"
            WITH RECURSIVE access_cte AS (
                SELECT vau.*
                FROM squadov.view_share_connections_access_users AS vau
                WHERE ($3::UUID IS NULL OR vau.match_uuid = $3)
                    AND ($4::UUID IS NULL OR vau.video_uuid = $4)
                    AND vau.user_id = $1
                UNION
                SELECT vau.*
                FROM squadov.view_share_connections_access_users AS vau
                INNER JOIN access_cte AS ac
                    ON ac.parent_connection_id = vau.id
            )
            SELECT EXISTS (
                SELECT 1
                FROM access_cte
                WHERE $2::BIGINT IS NULL OR source_user_id = $2
            ) AS "exists!"
            "#,
            dest_user_id,
            source_user_id,
            match_uuid,
            video_uuid,
        )
            .fetch_one(ex)
            .await?
            .exists
    )
}