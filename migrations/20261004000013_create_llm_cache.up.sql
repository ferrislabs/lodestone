CREATE TABLE llm_cache (
    cache_key        BYTEA       PRIMARY KEY,
    profile_id       UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision > 0),
    prompt           TEXT        NOT NULL,
    domain           TEXT        NOT NULL,
    output           JSONB       NOT NULL,
    model            TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL
);

CREATE INDEX llm_cache_profile_idx ON llm_cache (profile_id, profile_revision);
