DROP INDEX IF EXISTS lead_qualification_state_summary_provider_idx;

ALTER TABLE lead_qualification_state
    DROP COLUMN summary_provider_id;

DROP TABLE lead_agent_retries;
