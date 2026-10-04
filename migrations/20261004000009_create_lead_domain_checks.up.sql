CREATE TABLE lead_domain_checks (
    domain     TEXT        NOT NULL,
    source     TEXT        NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (domain, source)
);

CREATE INDEX leads_domain_idx ON leads (domain);
