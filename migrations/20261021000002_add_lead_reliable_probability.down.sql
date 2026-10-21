DROP INDEX leads_relevance_idx;

ALTER TABLE leads
    DROP COLUMN reliable_probability;
