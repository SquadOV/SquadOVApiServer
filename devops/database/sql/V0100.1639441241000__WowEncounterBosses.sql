CREATE TABLE IF NOT EXISTS wow_encounter_bosses (
    encounter_id BIGINT NOT NULL,
    npc_id BIGINT NOT NULL,
    name VARCHAR NOT NULL,
    UNIQUE(encounter_id, npc_id)
);