CREATE TABLE jobs (
    id           UUID        PRIMARY KEY,
    kind         TEXT        NOT NULL,
    payload      JSONB       NOT NULL,
    status       TEXT        NOT NULL
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed')),
    attempts     INTEGER     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER     NOT NULL CHECK (max_attempts > 0),
    run_at       TIMESTAMPTZ NOT NULL,
    lease_token  UUID,
    lease_until  TIMESTAMPTZ,
    last_error   TEXT,
    created_at   TIMESTAMPTZ NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL,
    CHECK ((status = 'running') = (lease_token IS NOT NULL AND lease_until IS NOT NULL))
);

CREATE INDEX jobs_claimable_idx ON jobs (run_at) WHERE status = 'pending';
CREATE INDEX jobs_expired_lease_idx ON jobs (lease_until) WHERE status = 'running';
