CREATE TABLE combat_logs(
    partition_id VARCHAR PRIMARY KEY,
    start_time TIMESTAMPTZ NOT NULL,
    owner_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE
);