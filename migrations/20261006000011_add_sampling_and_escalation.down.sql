ALTER TABLE lead_qualification_state
    DROP COLUMN escalated_revision;

ALTER TABLE qualification_results
    DROP COLUMN raw_samples,
    DROP COLUMN escalated;
