CREATE TABLE lead_requalifications (
    lead_id         UUID        PRIMARY KEY REFERENCES leads (id) ON DELETE CASCADE,
    requalify_after TIMESTAMPTZ,
    marked_at       TIMESTAMPTZ,
    forced          BOOLEAN     NOT NULL DEFAULT FALSE,
    fresh_calls     BOOLEAN     NOT NULL DEFAULT FALSE,
    daily_count     INTEGER     NOT NULL DEFAULT 0 CHECK (daily_count >= 0),
    counted_on      DATE
);

CREATE INDEX lead_requalifications_due_idx ON lead_requalifications (requalify_after)
    WHERE requalify_after IS NOT NULL;

ALTER TABLE lead_ecosystem_modules
    ADD COLUMN content_hash TEXT,
    ADD COLUMN changed_at   TIMESTAMPTZ;

ALTER TABLE leads
    ADD COLUMN excluded_reason TEXT,
    ADD COLUMN excluded_at     TIMESTAMPTZ;

ALTER TABLE listeners
    ADD COLUMN last_error_at TIMESTAMPTZ;

UPDATE listeners SET last_error_at = updated_at WHERE last_error IS NOT NULL;
