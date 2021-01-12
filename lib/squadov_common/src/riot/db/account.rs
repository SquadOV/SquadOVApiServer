use crate::{
    SquadOvError,
    riot::RiotAccount
};
use sqlx::{Executor, Postgres};

pub async fn get_user_riot_account_gamename_tagline<'a, T>(ex: T, user_id: i64, game_name: &str, tag_line: &str, game: &str) -> Result<RiotAccount, SquadOvError>
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
            AND ra.game = $4
        ",
        user_id,
        game_name,
        tag_line,
        game
    )
        .fetch_one(ex)
        .await?)
}

pub async fn get_user_riot_account<'a, T>(ex: T, user_id: i64, puuid: &str, game: &str) -> Result<RiotAccount, SquadOvError>
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
            AND ra.game = $3
        ",
        user_id,
        puuid,
        game
    )
        .fetch_one(ex)
        .await?)
}

pub async fn list_riot_accounts_for_user<'a, T>(ex: T, user_id: i64, game: &str) -> Result<Vec<RiotAccount>, SquadOvError>
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
            AND ra.game = $2
        ",
        user_id,
        game
    )
        .fetch_all(ex)
        .await?)
}

pub async fn store_riot_account<'a, T>(ex: T, account: &RiotAccount, game: &str) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.riot_accounts (
            puuid,
            game_name,
            tag_line,
            game
        )
        VALUES (
            $1,
            $2,
            $3,
            $4
        )
        ON CONFLICT (puuid, game) DO UPDATE
            SET game_name = EXCLUDED.game_name,
                tag_line = EXCLUDED.tag_line
        ",
        account.puuid,
        account.game_name,
        account.puuid,
        game,
    )
        .execute(ex)
        .await?;
    Ok(())
}