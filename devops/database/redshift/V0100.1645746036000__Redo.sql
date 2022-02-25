DROP TABLE wow_matches CASCADE;
DROP TABLE wow_match_combatants CASCADE;

CREATE TABLE wow_matches (
    id VARCHAR DISTKEY PRIMARY KEY,
    tm TIMESTAMP SORTKEY NOT NULL,
    build VARCHAR NOT NULL,
    info SUPER NOT NULL,
    match_type VARCHAR NOT NULL
);

CREATE TABLE wow_match_combatants (
    match_id VARCHAR DISTKEY REFERENCES wow_matches(id),
    player_guid VARCHAR NOT NULL,
    spec_id INTEGER NOT NULL,
    class_id INTEGER,
    rating INTEGER NOT NULL,
    team INTEGER NOT NULL,
    items SUPER NOT NULL,
    talents SUPER,
    covenant SUPER
);