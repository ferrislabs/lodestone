CREATE TABLE lead_agent_retries (
    lead_id          UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    kind             TEXT        NOT NULL CHECK (kind IN ('qualification', 'scoring')),
    profile_revision INTEGER     NOT NULL CHECK (profile_revision >= 1),
    agent_id         TEXT        NOT NULL,
    provider_id      UUID,
    attempts         INTEGER     NOT NULL CHECK (attempts >= 0),
    next_attempt_at  TIMESTAMPTZ,
    error            TEXT        NOT NULL,
    updated_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (lead_id, kind)
);

CREATE INDEX lead_agent_retries_provider_idx ON lead_agent_retries (provider_id);

ALTER TABLE lead_qualification_state
    ADD COLUMN summary_provider_id UUID;

CREATE INDEX lead_qualification_state_summary_provider_idx
    ON lead_qualification_state (summary_provider_id)
    WHERE summary_provider_id IS NOT NULL;

UPDATE lead_qualification_state state
SET scoring_for = NULL
WHERE EXISTS (
    SELECT 1
    FROM scoring_results result
    WHERE result.lead_id = state.lead_id
      AND result.profile_revision = state.profile_revision
      AND result.status = 'invalid'
      AND result.error LIKE 'Échec de l''appel au modèle%'
);

UPDATE lead_qualification_state state
SET sources = ARRAY[]::TEXT[]
WHERE EXISTS (
    SELECT 1
    FROM qualification_results result
    WHERE result.lead_id = state.lead_id
      AND result.profile_revision = state.profile_revision
      AND result.status = 'invalid'
      AND result.error LIKE 'Échec de l''appel au modèle%'
);
