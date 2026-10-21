ALTER TABLE candidate_hosts DROP CONSTRAINT candidate_hosts_origin_check;
ALTER TABLE candidate_hosts
    ADD CONSTRAINT candidate_hosts_origin_check
        CHECK (origin IN ('import', 'certificate_transparency', 'lead'));

CREATE TABLE lead_enrichments (
    lead_id UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    step    TEXT        NOT NULL CHECK (step IN ('http_probe', 'company_jobs')),
    ran_at  TIMESTAMPTZ,
    PRIMARY KEY (lead_id, step)
);
