UPDATE lead_qualification_state SET verdict_reason = NULL WHERE verdict_reason = 'awaiting_scoring';

ALTER TABLE lead_qualification_state
    DROP CONSTRAINT lead_qualification_state_verdict_reason_check,
    ADD CONSTRAINT lead_qualification_state_verdict_reason_check
        CHECK (verdict_reason IN ('single_angle', 'weak_evidence', 'non_production', 'excluded')),
    DROP COLUMN scoring_for,
    DROP COLUMN scoring_revision,
    DROP COLUMN verdict_probabilities,
    DROP COLUMN verdict_source,
    DROP COLUMN indicator_cap,
    DROP COLUMN indicator_confidence;

ALTER TABLE leads DROP COLUMN llm_score;

DROP TABLE scoring_results;
