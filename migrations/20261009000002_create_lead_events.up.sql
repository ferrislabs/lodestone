CREATE TABLE lead_events (
    id          UUID        PRIMARY KEY,
    lead_id     UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    profile_id  UUID        NOT NULL,
    round       INTEGER     NOT NULL CHECK (round >= 0),
    step        TEXT        NOT NULL CHECK (step IN (
        'detection', 'exclusion', 'inspection', 'qualification', 'escalation', 'scoring',
        'combination', 'aggregation', 'requalification', 'stop', 'decision'
    )),
    kind        TEXT        NOT NULL CHECK (kind <> ''),
    status      TEXT        NOT NULL CHECK (status IN ('started', 'succeeded', 'failed', 'skipped', 'waiting')),
    actor       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    detail      JSONB       NOT NULL DEFAULT '{}'::jsonb,
    started_at  TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ,
    error       TEXT,
    dedupe_key  TEXT        NOT NULL UNIQUE
);

CREATE INDEX lead_events_lead_idx ON lead_events (lead_id, started_at);
CREATE INDEX lead_events_started_idx ON lead_events (started_at);
CREATE INDEX lead_events_rounds_idx ON lead_events (lead_id, round) WHERE kind = 'round_started';
