DROP INDEX leads_confidence_idx;

ALTER TABLE lead_qualification_state
    DROP COLUMN summarised_at;

ALTER TABLE assessments
    DROP COLUMN evidence_verified,
    DROP COLUMN verdict_narrative;
