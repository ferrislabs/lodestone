ALTER TABLE leads
    ADD COLUMN reliable_probability DOUBLE PRECISION
        CHECK (reliable_probability >= 0 AND reliable_probability <= 100);

UPDATE leads
SET reliable_probability = 0
FROM lead_qualification_state state
WHERE state.lead_id = leads.id
  AND state.verdict_reason = 'excluded';

UPDATE leads
SET reliable_probability = LEAST(
    100,
    GREATEST(0, ROUND((state.verdict_probabilities ->> 'fiable')::numeric * 100, 1))
)::double precision
FROM lead_qualification_state state
WHERE state.lead_id = leads.id
  AND leads.reliable_probability IS NULL
  AND jsonb_typeof(state.verdict_probabilities -> 'fiable') = 'number';

CREATE INDEX leads_relevance_idx
    ON leads (profile_id, reliable_probability DESC NULLS LAST, score DESC);
