ALTER TABLE user_autoshare_common_settings
ADD COLUMN share_on_join2 BOOLEAN NOT NULL DEFAULT FALSE,
DROP COLUMN share_on_join;

ALTER TABLE user_autoshare_common_settings
RENAME COLUMN share_on_join2 TO share_on_join;