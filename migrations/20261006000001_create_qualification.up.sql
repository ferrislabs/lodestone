CREATE TABLE qualification_results (
    id               UUID             PRIMARY KEY,
    lead_id          UUID             NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    profile_id       UUID             NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    profile_revision INTEGER          NOT NULL CHECK (profile_revision > 0),
    agent_id         TEXT             NOT NULL,
    provider_id      UUID,
    model            TEXT             NOT NULL,
    prompt_hash      TEXT             NOT NULL,
    status           TEXT             NOT NULL CHECK (status IN ('valid', 'invalid')),
    answers          JSONB            NOT NULL DEFAULT '{}'::jsonb,
    agent_score      DOUBLE PRECISION CHECK (agent_score >= 0 AND agent_score <= 1),
    error            TEXT,
    created_at       TIMESTAMPTZ      NOT NULL,
    UNIQUE (lead_id, profile_revision, agent_id),
    CHECK ((status = 'valid') = (agent_score IS NOT NULL))
);

CREATE INDEX qualification_results_lead_idx ON qualification_results (lead_id);

CREATE TABLE lead_qualification_state (
    lead_id          UUID        PRIMARY KEY REFERENCES leads (id) ON DELETE CASCADE,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision > 0),
    sources          TEXT[]      NOT NULL,
    completed_at     TIMESTAMPTZ NOT NULL
);

ALTER TABLE leads
    ADD COLUMN confidence   DOUBLE PRECISION CHECK (confidence >= 0 AND confidence <= 100),
    ADD COLUMN verdict      TEXT CHECK (verdict IN ('fiable', 'a_verifier', 'ecarte')),
    ADD COLUMN qualified_at TIMESTAMPTZ;
