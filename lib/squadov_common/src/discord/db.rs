use crate::{
    SquadOvError,
    discord::{
        DiscordUser,
        oauth::DiscordOAuthToken,
    },
};
use sqlx::{Executor, Postgres};


pub async fn store_discord_user<'a, T>(ex: T, user: &DiscordUser) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.discord_users (
            id,
            username,
            discriminator,
            avatar
        ) VALUES (
            $1,
            $2,
            $3,
            $4
        )
        ON CONFLICT (id) DO UPDATE SET
            username = EXCLUDED.username,
            discriminator = EXCLUDED.discriminator,
            avatar = EXCLUDED.avatar
        ",
        user.id.parse::<i64>()?,
        &user.username,
        &user.discriminator,
        user.avatar,
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn link_discord_user_to_squadv<'a, T>(ex: T, id: i64, discord_id: i64, token: &DiscordOAuthToken) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        INSERT INTO squadov.user_discord_link (
            user_id,
            discord_snowflake,
            access_token,
            refresh_token,
            token_expires
        ) VALUES (
            $1,
            $2,
            $3,
            $4,
            $5
        )
        ON CONFLICT (user_id, discord_snowflake) DO UPDATE SET
            access_token = EXCLUDED.access_token,
            refresh_token = EXCLUDED.refresh_token,
            token_expires = EXCLUDED.token_expires
        ",
        id,
        discord_id,
        &token.access_token,
        &token.refresh_token,
        token.expiration_time(),
    )
        .execute(ex)
        .await?;
    Ok(())
}

pub async fn update_access_refresh_tokens<'a, T>(ex: T, id: i64, access_token: &str, token: &DiscordOAuthToken) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        UPDATE squadov.user_discord_link
        SET access_token = $3,
            refresh_token = $4,
            token_expires = $5
        WHERE user_id = $1
            AND access_token = $2
        ",
        id,
        access_token,
        &token.access_token,
        &token.refresh_token,
        token.expiration_time(),
    )
    .execute(ex)
    .await?;
    Ok(())
}

pub async fn find_discord_accounts_for_user<'a, T>(ex: T, id: i64) -> Result<Vec<DiscordUser>, SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    Ok(
        sqlx::query_as!(
            DiscordUser,
            r#"
            SELECT DISTINCT
                du.id::VARCHAR AS "id!",
                du.username,
                du.discriminator,
                du.avatar
            FROM squadov.user_discord_link AS udl
            INNER JOIN squadov.discord_users AS du
                ON du.id = udl.discord_snowflake
            WHERE udl.user_id = $1
            "#,
            id,
        )
            .fetch_all(ex)
            .await?
    )
}

pub async fn unlink_discord_account_for_user<'a, T>(ex: T, id: i64, discord_id: i64) -> Result<(), SquadOvError>
where
    T: Executor<'a, Database = Postgres>
{
    sqlx::query!(
        "
        DELETE FROM squadov.user_discord_link
        WHERE user_id = $1
            AND discord_snowflake = $2
        ",
        id,
        discord_id,
    )
        .execute(ex)
        .await?;
    Ok(())
}