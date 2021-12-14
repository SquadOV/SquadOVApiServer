ALTER TABLE wow_match_view_character_presence
ADD COLUMN current_hp BIGINT,
ADD COLUMN max_hp BIGINT,
ADD COLUMN creature_id BIGINT;