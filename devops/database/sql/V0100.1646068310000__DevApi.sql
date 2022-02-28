CREATE TABLE devapi_keys (
    api_key UUID PRIMARY KEY,
    user_id BIGINT NOT NULL
);

CREATE INDEX ON devapi_keys(user_id);