ALTER TABLE vod_metadata
ADD COLUMN bucket VARCHAR NOT NULL DEFAULT 'gs://squadov-vod-bucket';