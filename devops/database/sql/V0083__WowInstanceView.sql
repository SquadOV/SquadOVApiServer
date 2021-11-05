
CREATE TABLE wow_instance_view (
    view_id UUID UNIQUE NOT NULL REFERENCES wow_match_view(id) ON DELETE CASCADE,
    instance_id INTEGER NOT NULL,
    instance_name VARCHAR NOT NULL,
    instance_type INTEGER NOT NULL,
    is_converted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE EXTENSION IF NOT EXISTS intarray; 
CREATE TABLE new_wow_instances (
    match_uuid UUID UNIQUE NOT NULL REFERENCES matches (uuid) ON DELETE CASCADE,
    tr TSTZRANGE NOT NULL,
    instance_id INTEGER NOT NULL,
    instance_type INTEGER NOT NULL,
    players INTEGER[] NOT NULL,
    EXCLUDE USING GIST (match_uuid WITH <>, instance_id WITH =, instance_type WITH =, players WITH &&, tr WITH &&)
);