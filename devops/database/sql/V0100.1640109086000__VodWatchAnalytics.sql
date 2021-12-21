CREATE TABLE vod_watch_analytics(
    video_uuid UUID NOT NULL REFERENCES vods(video_uuid) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    start_seconds BIGINT NOT NULL,
    end_seconds BIGINT NOT NULL,
    tm TIMESTAMPTZ NOT NULL
);

CREATE INDEX ON vod_watch_analytics(video_uuid, user_id);
CREATE INDEX ON vod_watch_analytics(user_id, video_uuid);
CREATE INDEX ON vod_watch_analytics(tm);