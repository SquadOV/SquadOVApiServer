CREATE TABLE new_wow_user_character_cache (
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    unit_guid VARCHAR NOT NULL,
    unit_name VARCHAR NOT NULL,
    spec_id INTEGER,
    class_id INTEGER,
    items INTEGER[],
    cache_time TIMESTAMPTZ NOT NULL,
    build_version VARCHAR NOT NULL,
    UNIQUE(user_id, unit_guid)
);

CREATE INDEX ON new_wow_user_character_cache(unit_guid);

INSERT INTO new_wow_user_character_cache (
    user_id,
    unit_guid,
    unit_name,
    spec_id,
    class_id,
    items,
    cache_time,
    build_version
)
SELECT
    wucc.user_id,
    wucc.unit_guid,
    wcp.unit_name,
    wvc.spec_id,
    wvc.class_id,
    ARRAY_AGG(wci.ilvl ORDER BY wci.idx ASC),
    wucc.cache_time,
    wmv.build_version
FROM wow_user_character_cache AS wucc
INNER JOIN wow_match_view_combatants AS wvc
    ON wvc.event_id = wucc.event_id
INNER JOIN wow_match_view_character_presence AS wcp
    ON wcp.character_id = wvc.character_id
INNER JOIN squadov.wow_match_view AS wmv
    ON wmv.id = wcp.view_id
LEFT JOIN wow_match_view_combatant_items AS wci
    ON wci.event_id = wvc.event_id
GROUP BY wucc.user_id, wucc.unit_guid, wcp.unit_name, wvc.spec_id, wvc.class_id, wucc.cache_time, wmv.build_version;

DROP TABLE wow_user_character_cache CASCADE;

ALTER TABLE new_wow_user_character_cache
RENAME TO wow_user_character_cache;