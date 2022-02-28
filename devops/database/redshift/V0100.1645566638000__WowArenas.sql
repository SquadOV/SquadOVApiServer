CREATE TABLE wow_matches (
    id VARCHAR DISTKEY PRIMARY KEY,
    tm TIMESTAMPTZ SORTKEY NOT NULL,
    build VARCHAR NOT NULL,
    info SUPER NOT NULL
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