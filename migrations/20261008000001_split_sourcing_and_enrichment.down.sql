DROP TABLE lead_enrichments;

DELETE FROM candidate_hosts WHERE origin = 'lead';
ALTER TABLE candidate_hosts DROP CONSTRAINT candidate_hosts_origin_check;
ALTER TABLE candidate_hosts
    ADD CONSTRAINT candidate_hosts_origin_check
        CHECK (origin IN ('import', 'certificate_transparency'));
