ALTER TABLE lead_qualification_state
    DROP CONSTRAINT lead_qualification_state_verdict_reason_check,
    ADD CONSTRAINT lead_qualification_state_verdict_reason_check
        CHECK (verdict_reason IN ('single_angle', 'weak_evidence', 'non_production', 'excluded')),
    ADD COLUMN exclusions JSONB NOT NULL DEFAULT '[]'::jsonb;
