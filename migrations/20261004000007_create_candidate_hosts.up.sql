CREATE TABLE candidate_hosts (
    host          TEXT        PRIMARY KEY,
    domain        TEXT        NOT NULL,
    origin        TEXT        NOT NULL CHECK (origin IN ('import', 'certificate_transparency')),
    first_seen_at TIMESTAMPTZ NOT NULL,
    last_seen_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX candidate_hosts_domain_idx ON candidate_hosts (domain);

CREATE TABLE host_source_checks (
    host       TEXT        NOT NULL REFERENCES candidate_hosts (host) ON DELETE CASCADE,
    source     TEXT        NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (host, source)
);
