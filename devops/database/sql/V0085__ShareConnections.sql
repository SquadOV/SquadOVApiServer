CREATE INDEX CONCURRENTLY ON share_match_vod_connections(match_uuid, dest_squad_id, source_user_id);
CREATE INDEX CONCURRENTLY ON share_match_vod_connections(match_uuid, dest_user_id, source_user_id);

CREATE INDEX CONCURRENTLY ON share_match_vod_connections(video_uuid, dest_squad_id, source_user_id);
CREATE INDEX CONCURRENTLY ON share_match_vod_connections(video_uuid, dest_user_id, source_user_id);