CREATE TABLE valorant_match_computed_data (
    match_uuid UUID UNIQUE NOT NULL REFERENCES valorant_matches(match_uuid) ON DELETE CASCADE,
    t0_agents VARCHAR NOT NULL,
    t1_agents VARCHAR NOT NULL
);

CREATE TABLE valorant_match_pov_computed_data (
    match_uuid UUID NOT NULL REFERENCES valorant_matches(match_uuid) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pov_agent VARCHAR NOT NULL,
    rank INTEGER NOT NULL,
    key_events INTEGER[] NOT NULL,
    winner BOOLEAN NOT NULL,
    UNIQUE(match_uuid, user_id)
);