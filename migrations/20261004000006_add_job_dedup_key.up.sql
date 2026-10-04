ALTER TABLE jobs ADD COLUMN dedup_key TEXT;

CREATE UNIQUE INDEX jobs_active_dedup_key_idx ON jobs (dedup_key)
    WHERE dedup_key IS NOT NULL AND status IN ('pending', 'running');
