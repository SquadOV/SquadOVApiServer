ALTER TABLE share_tokens
ADD COLUMN meta_user_id BIGINT REFERENCES users(id) ON DELETE SET NULL;