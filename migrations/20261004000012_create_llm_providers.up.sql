CREATE TABLE llm_providers (
    id                     UUID        PRIMARY KEY,
    name                   TEXT        NOT NULL UNIQUE,
    kind                   TEXT        NOT NULL CHECK (kind IN ('anthropic', 'mistral', 'openai_compatible')),
    base_url               TEXT        NOT NULL,
    model                  TEXT        NOT NULL,
    timeout_ms             INTEGER     NOT NULL CHECK (timeout_ms > 0),
    max_retries            SMALLINT    NOT NULL CHECK (max_retries >= 0),
    input_cents_per_mtok   INTEGER     NOT NULL CHECK (input_cents_per_mtok >= 0),
    output_cents_per_mtok  INTEGER     NOT NULL CHECK (output_cents_per_mtok >= 0),
    api_key_nonce          BYTEA,
    api_key_ciphertext     BYTEA,
    enabled                BOOLEAN     NOT NULL DEFAULT true,
    created_at             TIMESTAMPTZ NOT NULL,
    updated_at             TIMESTAMPTZ NOT NULL,
    CHECK ((api_key_nonce IS NULL) = (api_key_ciphertext IS NULL))
);

CREATE TABLE llm_tiers (
    tier        TEXT        PRIMARY KEY CHECK (tier IN ('triage', 'qualification')),
    provider_id UUID        NOT NULL REFERENCES llm_providers (id) ON DELETE RESTRICT,
    updated_at  TIMESTAMPTZ NOT NULL
);
