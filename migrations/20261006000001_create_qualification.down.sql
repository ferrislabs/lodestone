ALTER TABLE leads
    DROP COLUMN qualified_at,
    DROP COLUMN verdict,
    DROP COLUMN confidence;

DROP TABLE lead_qualification_state;
DROP TABLE qualification_results;
