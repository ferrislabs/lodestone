CREATE TABLE profile_trials (
    id            UUID        PRIMARY KEY,
    profile_id    UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    base_revision INTEGER     NOT NULL CHECK (base_revision > 0),
    definition    JSONB       NOT NULL,
    status        TEXT        NOT NULL CHECK (status IN ('pending', 'running', 'done', 'failed', 'budget_exhausted')),
    sample_size   INTEGER     NOT NULL CHECK (sample_size BETWEEN 1 AND 50),
    budget_cents  INTEGER     NOT NULL CHECK (budget_cents BETWEEN 1 AND 2000),
    spent_cents   DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK (spent_cents >= 0),
    error         TEXT,
    created_by    UUID        REFERENCES users (id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL,
    started_at    TIMESTAMPTZ,
    finished_at   TIMESTAMPTZ
);

CREATE INDEX profile_trials_profile_idx ON profile_trials (profile_id, created_at DESC);
CREATE UNIQUE INDEX profile_trials_live_idx ON profile_trials (profile_id) WHERE status IN ('pending', 'running');

CREATE TABLE profile_trial_leads (
    trial_id           UUID             NOT NULL REFERENCES profile_trials (id) ON DELETE CASCADE,
    lead_id            UUID             NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    label              TEXT             NOT NULL CHECK (label IN ('validated', 'rejected')),
    current_verdict    TEXT,
    current_confidence DOUBLE PRECISION,
    trial_verdict      TEXT,
    trial_confidence   DOUBLE PRECISION,
    error              TEXT,
    tried_at           TIMESTAMPTZ      NOT NULL,
    PRIMARY KEY (trial_id, lead_id)
);
