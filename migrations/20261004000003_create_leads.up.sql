CREATE TABLE leads (
    id            UUID             PRIMARY KEY,
    profile_id    UUID             NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    domain        TEXT             NOT NULL,
    status        TEXT             NOT NULL
        CHECK (status IN ('new', 'qualified', 'validated', 'rejected', 'contacted')),
    score         DOUBLE PRECISION NOT NULL DEFAULT 0,
    first_seen_at TIMESTAMPTZ      NOT NULL,
    last_seen_at  TIMESTAMPTZ      NOT NULL,
    updated_at    TIMESTAMPTZ      NOT NULL,
    UNIQUE (profile_id, domain),
    CHECK (first_seen_at <= last_seen_at)
);

CREATE INDEX leads_profile_status_idx ON leads (profile_id, status);

CREATE TABLE signals (
    id          UUID             PRIMARY KEY,
    lead_id     UUID             NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    kind        TEXT             NOT NULL,
    value       TEXT,
    source      TEXT             NOT NULL,
    raw         JSONB            NOT NULL,
    provenance  TEXT             NOT NULL,
    strength    DOUBLE PRECISION NOT NULL CHECK (strength >= 0 AND strength <= 1),
    observed_at TIMESTAMPTZ      NOT NULL
);

CREATE INDEX signals_lead_idx ON signals (lead_id, observed_at);

CREATE TABLE contact_emails (
    id         UUID             PRIMARY KEY,
    lead_id    UUID             NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    address    TEXT             NOT NULL,
    provenance TEXT             NOT NULL,
    method     TEXT             NOT NULL
        CHECK (method IN ('public_page', 'security_txt', 'role_guess', 'llm_extraction')),
    confidence DOUBLE PRECISION NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    kind       TEXT             NOT NULL CHECK (kind IN ('role', 'nominative')),
    found_at   TIMESTAMPTZ      NOT NULL,
    UNIQUE (lead_id, address)
);

CREATE TABLE assessments (
    id               UUID        PRIMARY KEY,
    lead_id          UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision > 0),
    summary          TEXT        NOT NULL,
    approach_angle   TEXT        NOT NULL,
    evidence         JSONB       NOT NULL,
    model            TEXT        NOT NULL,
    prompt_version   TEXT        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL
);

CREATE INDEX assessments_lead_idx ON assessments (lead_id, created_at DESC);

CREATE TABLE runs (
    id          UUID        PRIMARY KEY,
    profile_id  UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    source      TEXT        NOT NULL,
    status      TEXT        NOT NULL CHECK (status IN ('running', 'succeeded', 'failed')),
    started_at  TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    volume      BIGINT      NOT NULL DEFAULT 0 CHECK (volume >= 0),
    signals     BIGINT      NOT NULL DEFAULT 0 CHECK (signals >= 0),
    error       TEXT,
    CHECK ((status = 'running') = (finished_at IS NULL))
);

CREATE INDEX runs_started_idx ON runs (started_at DESC);

CREATE TABLE suppression_list (
    id         UUID        PRIMARY KEY,
    kind       TEXT        NOT NULL CHECK (kind IN ('domain', 'email')),
    value      TEXT        NOT NULL,
    reason     TEXT,
    created_by UUID        REFERENCES users (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (kind, value)
);
