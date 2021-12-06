CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE TABLE tags (
    tag_id BIGSERIAL PRIMARY KEY,
    tag VARCHAR UNIQUE NOT NULL
);

CREATE INDEX ON tags USING GIN(tag gin_trgm_ops);

CREATE TABLE user_vod_tags (
    video_uuid UUID NOT NULL REFERENCES vods(video_uuid) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(tag_id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tm TIMESTAMPTZ NOT NULL,
    UNIQUE(video_uuid, tag_id, user_id)
);