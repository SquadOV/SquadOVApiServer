CREATE TABLE discord_users (
    id BIGINT PRIMARY KEY,
    username VARCHAR NOT NULL,
    discriminator VARCHAR NOT NULL,
    avatar VARCHAR
);

CREATE TABLE user_discord_link (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    discord_snowflake BIGINT NOT NULL REFERENCES discord_users(id) ON DELETE CASCADE,
    access_token VARCHAR NOT NULL,
    refresh_token VARCHAR NOT NULL,
    token_expires TIMESTAMPTZ NOT NULL,
    UNIQUE(user_id, discord_snowflake)
);