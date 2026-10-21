CREATE TABLE lead_ecosystem (
    domain      TEXT        NOT NULL,
    module      TEXT        NOT NULL CHECK (module IN (
        'official_identity', 'legal_life', 'public_contracts', 'website_stack',
        'dns_email', 'hiring_activity', 'recent_activity'
    )),
    finding_key TEXT        NOT NULL,
    kind        TEXT        NOT NULL,
    label       TEXT        NOT NULL,
    value       TEXT        NOT NULL,
    source_url  TEXT        NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    details     JSONB       NOT NULL DEFAULT 'null'::jsonb,
    PRIMARY KEY (domain, module, finding_key)
);

CREATE TABLE lead_ecosystem_modules (
    domain      TEXT        NOT NULL,
    module      TEXT        NOT NULL CHECK (module IN (
        'official_identity', 'legal_life', 'public_contracts', 'website_stack',
        'dns_email', 'hiring_activity', 'recent_activity'
    )),
    status      TEXT        NOT NULL CHECK (status IN ('pending', 'done', 'not_applicable', 'failed')),
    last_run    TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ NOT NULL,
    error       TEXT,
    PRIMARY KEY (domain, module)
);

CREATE INDEX lead_ecosystem_modules_due_idx ON lead_ecosystem_modules (module, next_run_at);

CREATE TABLE ecosystem_module_runs (
    id              UUID        PRIMARY KEY,
    module          TEXT        NOT NULL CHECK (module IN (
        'official_identity', 'legal_life', 'public_contracts', 'website_stack',
        'dns_email', 'hiring_activity', 'recent_activity'
    )),
    ran_at          TIMESTAMPTZ NOT NULL,
    leads_inspected BIGINT      NOT NULL CHECK (leads_inspected >= 0),
    findings_count  BIGINT      NOT NULL CHECK (findings_count >= 0),
    latest_example  JSONB
);

CREATE INDEX ecosystem_module_runs_module_idx ON ecosystem_module_runs (module, ran_at DESC);

ALTER TABLE company_dossiers DROP CONSTRAINT company_dossiers_state_check;
ALTER TABLE company_dossiers
    ADD CONSTRAINT company_dossiers_state_check
        CHECK (state IN ('pending', 'complete', 'partial', 'unreachable', 'hosting_platform'));
