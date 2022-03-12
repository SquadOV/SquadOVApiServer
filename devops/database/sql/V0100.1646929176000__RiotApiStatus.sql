CREATE TABLE riot_api_outage_status (
    game VARCHAR NOT NULL,
    region VARCHAR NOT NULL,
    down BOOLEAN NOT NULL,
    UNIQUE(game, region)
);