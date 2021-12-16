ALTER TABLE wow_match_view_events
ADD COLUMN source_flags BIGINT,
ADD COLUMN dest_flags BIGINT;