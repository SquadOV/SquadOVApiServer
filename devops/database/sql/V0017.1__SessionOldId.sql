ALTER TABLE user_sessions
ADD COLUMN old_id VARCHAR(36) UNIQUE;