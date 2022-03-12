use crate::{
    SquadOvError,
    riot::{RiotAccount}
};
use sqlx::{Executor, Postgres};
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub async fn get_user_riot_account_from_raw_puuid<'a, T>(ex: T, user_id: i64, raw_puuid: &str) -> Result<Option<RiotAccount>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            RiotAccount,
            r#"
            SELECT ra.puuid, ra.game_name, ra.tag_line
            FROM squadov.riot_accounts AS ra
            INNER JOIN squadov.riot_account_links AS ral
                ON ral.puuid = ra.puuid
            WHERE ra.raw_puuid = $1
                AND ral.user_id = $2
            "#,
            raw_puuid,
            user_id,
        )
            .fetch_optional(ex)
            .await?
    )
}

pub async fn associate_raw_puuid_with_puuid<'a, T>(ex: T, puuid: &str, raw_puuid: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.riot_accounts
        SET raw_puuid = $2
        WHERE puuid = $1
        ",
        puuid,
        raw_puuid,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn delete_riot_account<'a, T>(ex: T, puuid: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        DELETE FROM squadov.riot_accounts
        WHERE puuid = $1
        ",
        puuid,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn tick_riot_account_lol_backfill_time<'a, T>(ex: T, puuid: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.riot_accounts
        SET last_backfill_lol_time = NOW()
        WHERE puuid = $1
        ",
        puuid,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn tick_riot_puuid_tft_backfill_time<'a, T>(ex: T, puuid: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.riot_accounts
        SET last_backfill_tft_time = NOW()
        WHERE puuid = $1
        ",
        puuid,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn get_missing_riot_account_puuids<'a, T>(ex: T, puuids: &[String]) -> Result<Vec<String>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query!(
            r#"
            SELECT t.id AS "id!"
            FROM UNNEST($1::VARCHAR[]) AS t(id)
            LEFT JOIN squadov.riot_accounts AS ra
                ON ra.puuid = t.id
            WHERE ra.game_name IS NULL
                AND ra.tag_line IS NULL
            "#,
            puuids,
        )
            .fetch_all(ex)
            .await?
            .into_iter()
            .map(|x| {
                x.id
            })
            .collect()
    )
}


pub async fn get_user_riot_account_gamename_tagline<'a, T>(ex: T, user_id: i64, game_name: &str, tag_line: &str) -> Result<Option<RiotAccount>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(sqlx::query_as!(
        RiotAccount,
        "
        SELECT ra.puuid, ra.game_name, ra.tag_line
        FROM squadov.riot_accounts AS ra
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = ra.puuid
        WHERE ral.user_id = $1
            AND ra.game_name = $2
            AND ra.tag_line = $3
        ",
        user_id,
        game_name,
        tag_line,
    )
        .fetch_optional(ex)
        .await?)
}

pub async fn get_user_riot_account<'a, T>(ex: T, user_id: i64, puuid: &str) -> Result<RiotAccount, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(sqlx::query_as!(
        RiotAccount,
        "
        SELECT ra.puuid, ra.game_name, ra.tag_line
        FROM squadov.riot_accounts AS ra
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = ra.puuid
        WHERE ral.user_id = $1
            AND ra.puuid = $2
            AND ra.game_name IS NOT NULL
            AND ra.tag_line IS NOT NULL
        ",
        user_id,
        puuid,
    )
        .fetch_one(ex)
        .await?)
}

pub async fn get_riot_account_user_uuid<'a, T>(ex: T, puuid: &str) -> Result<Uuid, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(sqlx::query!(
        "
        SELECT u.uuid
        FROM squadov.riot_accounts AS ra
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = ra.puuid
        INNER JOIN squadov.users AS u
            ON u.id = ral.user_id
        WHERE ra.puuid = $1
            AND ra.game_name IS NOT NULL
            AND ra.tag_line IS NOT NULL
        ",
        puuid,
    )
        .fetch_one(ex)
        .await?
        .uuid
    )
}

pub async fn list_riot_accounts_for_user<'a, T>(ex: T, user_id: i64) -> Result<Vec<RiotAccount>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(sqlx::query_as!(
        RiotAccount,
        r#"
        SELECT ra.puuid, COALESCE(ra.game_name, ra.summoner_name) AS "game_name", COALESCE(ra.tag_line, '') AS "tag_line"
        FROM squadov.riot_accounts AS ra
        INNER JOIN squadov.riot_account_links AS ral
            ON ral.puuid = ra.puuid
        WHERE ral.user_id = $1
        "#,
        user_id,
    )
        .fetch_all(ex)
        .await?)
}

pub async fn store_riot_account<'a, T>(ex: T, account: &RiotAccount) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.riot_accounts (
            puuid,
            game_name,
            tag_line
        )
        VALUES (
            $1,
            $2,
            $3
        )
        ON CONFLICT (puuid) DO UPDATE
            SET game_name = EXCLUDED.game_name,
                tag_line = EXCLUDED.tag_line
        ",
        account.puuid,
        account.game_name,
        account.tag_line,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn link_riot_account_to_user<'a, T>(ex: T, puuid: &str, user_id: i64) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.riot_account_links (
            puuid,
            user_id
        )
        VALUES (
            $1,
            $2
        )
        ON CONFLICT DO NOTHING
        ",
        puuid,
        user_id
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn store_rso_for_riot_account<'a, T>(ex: T, puuid: &str, user_id: i64, access_token: &str, refresh_token: &str, expiration: &DateTime<Utc>) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.riot_account_links
        SET rso_access_token = $3,
            rso_refresh_token = $4,
            rso_expiration = $5
        WHERE puuid = $1 AND user_id = $2
        ",
        puuid,
        user_id,
        access_token,
        refresh_token,
        expiration
    )
        .execute(ex)
        .await?;
    Ok(())
}