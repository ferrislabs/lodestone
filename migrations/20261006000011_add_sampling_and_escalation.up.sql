ALTER TABLE qualification_results
    ADD COLUMN escalated   BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN raw_samples JSONB   NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE lead_qualification_state
    ADD COLUMN escalated_revision INTEGER CHECK (escalated_revision >= 1);
