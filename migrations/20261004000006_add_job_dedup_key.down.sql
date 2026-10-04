DROP INDEX jobs_active_dedup_key_idx;
ALTER TABLE jobs DROP COLUMN dedup_key;
