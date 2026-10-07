CREATE TABLE scoring_results (
    id               UUID        PRIMARY KEY,
    lead_id          UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    profile_id       UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision > 0),
    agent_id         TEXT        NOT NULL,
    provider_id      UUID,
    provider_name    TEXT        NOT NULL DEFAULT '',
    model            TEXT        NOT NULL,
    prompt_version   TEXT        NOT NULL,
    generation       JSONB       NOT NULL DEFAULT '{}'::jsonb,
    samples          INTEGER     NOT NULL DEFAULT 1 CHECK (samples >= 1),
    status           TEXT        NOT NULL CHECK (status IN ('valid', 'invalid')),
    criteria         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    score            SMALLINT    CHECK (score >= 0 AND score <= 100),
    verdict          JSONB,
    reasons          JSONB       NOT NULL DEFAULT '[]'::jsonb,
    error            TEXT,
    raw_samples      JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL,
    UNIQUE (lead_id, profile_revision, agent_id),
    CHECK ((status = 'valid') = (score IS NOT NULL AND verdict IS NOT NULL))
);

CREATE INDEX scoring_results_lead_idx ON scoring_results (lead_id);

ALTER TABLE leads
    ADD COLUMN llm_score SMALLINT CHECK (llm_score >= 0 AND llm_score <= 100);

ALTER TABLE lead_qualification_state
    ADD COLUMN indicator_confidence DOUBLE PRECISION
        CHECK (indicator_confidence >= 0 AND indicator_confidence <= 100),
    ADD COLUMN indicator_cap TEXT
        CHECK (indicator_cap IN ('single_angle', 'weak_evidence', 'non_production')),
    ADD COLUMN verdict_source TEXT NOT NULL DEFAULT 'indicators'
        CHECK (verdict_source IN ('indicators', 'scoring')),
    ADD COLUMN verdict_probabilities JSONB,
    ADD COLUMN scoring_revision INTEGER CHECK (scoring_revision >= 1),
    ADD COLUMN scoring_for TIMESTAMPTZ,
    DROP CONSTRAINT lead_qualification_state_verdict_reason_check,
    ADD CONSTRAINT lead_qualification_state_verdict_reason_check
        CHECK (verdict_reason IN (
            'single_angle', 'weak_evidence', 'non_production', 'excluded', 'awaiting_scoring'
        ));

UPDATE lead_qualification_state state
SET indicator_confidence = leads.confidence,
    indicator_cap = CASE
        WHEN state.verdict_reason IN ('single_angle', 'weak_evidence', 'non_production')
            THEN state.verdict_reason
    END
FROM leads
WHERE leads.id = state.lead_id;
