ALTER TABLE lead_qualification_state
    ADD COLUMN scoring_version INTEGER          NOT NULL DEFAULT 0,
    ADD COLUMN opinion         DOUBLE PRECISION CHECK (opinion >= 0 AND opinion <= 1),
    ADD COLUMN evidence        JSONB,
    ADD COLUMN verdict_reason  TEXT CHECK (verdict_reason IN ('single_angle', 'weak_evidence', 'non_production'));
