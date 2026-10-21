DROP TABLE lead_agent_stops;

UPDATE jobs SET status = 'failed' WHERE status = 'cancelled';

ALTER TABLE jobs
    DROP CONSTRAINT jobs_status_check,
    ADD CONSTRAINT jobs_status_check
        CHECK (status IN ('pending', 'running', 'succeeded', 'failed'));

UPDATE runs
SET status = 'failed', error = COALESCE(error, cancel_reason)
WHERE status = 'cancelled';

ALTER TABLE runs
    DROP CONSTRAINT runs_cancelled_reason_check,
    DROP COLUMN cancel_reason,
    DROP CONSTRAINT runs_status_check,
    ADD CONSTRAINT runs_status_check
        CHECK (status IN ('running', 'succeeded', 'failed'));
