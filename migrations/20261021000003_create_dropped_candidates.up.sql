CREATE TABLE dropped_candidates (
    id          UUID        PRIMARY KEY,
    profile_id  UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    agent_id    TEXT,
    source      TEXT        NOT NULL,
    kind        TEXT        NOT NULL,
    domain      TEXT        NOT NULL,
    title       TEXT        NOT NULL,
    excerpts    JSONB       NOT NULL DEFAULT '[]'::jsonb,
    provenance  TEXT        NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    reason      TEXT        NOT NULL CHECK (reason IN (
        'no_keyword', 'exclusion', 'filter_remote', 'filter_country', 'filter_contract'
    )),
    matched     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    excluded    JSONB       NOT NULL DEFAULT '[]'::jsonb,
    details     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    dropped_at  TIMESTAMPTZ NOT NULL,
    review      TEXT        CHECK (review IN ('rightly_dropped', 'should_have_kept')),
    reviewed_at TIMESTAMPTZ,
    reviewed_by UUID
);

CREATE UNIQUE INDEX dropped_candidates_identity_idx
    ON dropped_candidates (profile_id, (COALESCE(agent_id, '')), provenance);
CREATE INDEX dropped_candidates_recent_idx ON dropped_candidates (profile_id, dropped_at DESC);
CREATE INDEX dropped_candidates_reason_idx ON dropped_candidates (profile_id, reason);
CREATE INDEX dropped_candidates_unreviewed_idx ON dropped_candidates (dropped_at) WHERE review IS NULL;
