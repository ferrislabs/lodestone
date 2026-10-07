CREATE TABLE calibrations (
    profile_id   UUID             NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    kind         TEXT             NOT NULL CHECK (kind IN ('scoring', 'qualification')),
    agent_id     TEXT             NOT NULL,
    points       JSONB            NOT NULL DEFAULT '[]'::jsonb,
    n            INTEGER          NOT NULL CHECK (n >= 0),
    positives    INTEGER          NOT NULL CHECK (positives >= 0 AND positives <= n),
    brier_before DOUBLE PRECISION CHECK (brier_before >= 0 AND brier_before <= 1),
    brier_after  DOUBLE PRECISION CHECK (brier_after >= 0 AND brier_after <= 1),
    reliability  JSONB            NOT NULL DEFAULT '[]'::jsonb,
    fitted_at    TIMESTAMPTZ      NOT NULL,
    enabled      BOOLEAN          NOT NULL DEFAULT false,
    PRIMARY KEY (profile_id, kind, agent_id)
);
