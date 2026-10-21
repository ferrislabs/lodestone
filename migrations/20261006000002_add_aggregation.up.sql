ALTER TABLE assessments
    ADD COLUMN verdict_narrative TEXT    NOT NULL DEFAULT '',
    ADD COLUMN evidence_verified BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE lead_qualification_state
    ADD COLUMN summarised_at TIMESTAMPTZ;

CREATE INDEX leads_confidence_idx ON leads (profile_id, confidence DESC NULLS LAST, id);

DELETE FROM jobs WHERE kind = 'llm.qualify';
DELETE FROM runs WHERE source = 'llm_qualification';
UPDATE leads SET status = 'new' WHERE status = 'uncertain';
