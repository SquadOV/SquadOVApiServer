CREATE TABLE user_autoshare_settings (
    user_id BIGINT NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    share_on_join BOOLEAN NOT NULL DEFAULT TRUE
);

INSERT INTO user_autoshare_settings (user_id, share_on_join)
SELECT id, TRUE
FROM users;