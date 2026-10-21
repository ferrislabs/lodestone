CREATE TABLE source_provider_configurations (
    provider_key       TEXT        PRIMARY KEY,
    settings           JSONB       NOT NULL DEFAULT '{}'::jsonb,
    status             TEXT        NOT NULL CHECK (status IN ('untested', 'ready', 'failing')),
    last_test_at       TIMESTAMPTZ,
    last_test_message  TEXT,
    connected_account  JSONB,
    created_at         TIMESTAMPTZ NOT NULL,
    updated_at         TIMESTAMPTZ NOT NULL
);

CREATE TABLE source_provider_secrets (
    provider_key TEXT  NOT NULL REFERENCES source_provider_configurations (provider_key) ON DELETE CASCADE,
    name         TEXT  NOT NULL,
    nonce        BYTEA NOT NULL,
    ciphertext   BYTEA NOT NULL,
    PRIMARY KEY (provider_key, name)
);

CREATE TABLE source_provider_oauth_states (
    state                 TEXT        PRIMARY KEY,
    provider_key          TEXT        NOT NULL,
    user_id               UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_verifier_nonce   BYTEA       NOT NULL,
    code_verifier_cipher  BYTEA       NOT NULL,
    redirect_uri          TEXT        NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL,
    expires_at            TIMESTAMPTZ NOT NULL
);

CREATE INDEX source_provider_oauth_states_expires_idx ON source_provider_oauth_states (expires_at);
