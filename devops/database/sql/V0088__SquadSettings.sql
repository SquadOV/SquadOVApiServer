CREATE TABLE squad_sharing_games_filter (
    squad_id BIGINT NOT NULL REFERENCES squads(id) ON DELETE CASCADE,
    disabled_game INTEGER NOT NULL,
    UNIQUE(squad_id, disabled_game)
);

CREATE TABLE squad_sharing_wow_filters (
    squad_id BIGINT NOT NULL UNIQUE REFERENCES squads(id) ON DELETE CASCADE,
    disable_encounters BOOLEAN NOT NULL DEFAULT FALSE,
    disable_dungeons BOOLEAN NOT NULL DEFAULT FALSE,
    disable_keystones BOOLEAN NOT NULL DEFAULT FALSE,
    disable_arenas BOOLEAN NOT NULL DEFAULT FALSE,
    disable_bgs BOOLEAN NOT NULL DEFAULT FALSE
);