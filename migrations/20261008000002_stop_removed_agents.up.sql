ALTER TABLE runs
    DROP CONSTRAINT runs_status_check,
    ADD CONSTRAINT runs_status_check
        CHECK (status IN ('running', 'succeeded', 'failed', 'cancelled')),
    ADD COLUMN cancel_reason TEXT
        CHECK (cancel_reason IN ('agent_removed', 'agent_disabled', 'profile_deactivated', 'profile_deleted')),
    ADD CONSTRAINT runs_cancelled_reason_check
        CHECK ((status = 'cancelled') = (cancel_reason IS NOT NULL));

ALTER TABLE jobs
    DROP CONSTRAINT jobs_status_check,
    ADD CONSTRAINT jobs_status_check
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed', 'cancelled'));

CREATE TABLE lead_agent_stops (
    lead_id          UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    kind             TEXT        NOT NULL
        CHECK (kind IN ('sourcing', 'enrichment', 'qualification', 'scoring', 'aggregator')),
    agent_id         TEXT        NOT NULL,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision >= 1),
    reason           TEXT        NOT NULL
        CHECK (reason IN ('agent_removed', 'agent_disabled', 'profile_deactivated', 'profile_deleted')),
    stopped_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (lead_id, kind, agent_id)
);
