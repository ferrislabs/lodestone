ALTER TABLE listeners DROP COLUMN last_error_at;

ALTER TABLE leads
    DROP COLUMN excluded_at,
    DROP COLUMN excluded_reason;

ALTER TABLE lead_ecosystem_modules
    DROP COLUMN changed_at,
    DROP COLUMN content_hash;

DROP TABLE lead_requalifications;
