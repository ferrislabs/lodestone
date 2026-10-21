UPDATE lead_qualification_state SET verdict_reason = NULL WHERE verdict_reason = 'excluded';
ALTER TABLE lead_qualification_state
    DROP COLUMN exclusions,
    DROP CONSTRAINT lead_qualification_state_verdict_reason_check,
    ADD CONSTRAINT lead_qualification_state_verdict_reason_check
        CHECK (verdict_reason IN ('single_angle', 'weak_evidence', 'non_production'));
