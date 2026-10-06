CREATE TABLE company_dossiers (
    domain          TEXT        PRIMARY KEY,
    state           TEXT        NOT NULL CHECK (state IN ('pending', 'complete', 'partial', 'unreachable')),
    siren           TEXT        CHECK (siren ~ '^[0-9]{9}$'),
    official        JSONB,
    pages           JSONB       NOT NULL DEFAULT '[]'::jsonb,
    content_hash    TEXT,
    content_at      TIMESTAMPTZ,
    built_at        TIMESTAMPTZ,
    attempts        INTEGER     NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TIMESTAMPTZ NOT NULL,
    error           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX company_dossiers_next_attempt_idx ON company_dossiers (next_attempt_at);

ALTER TABLE lead_qualification_state ADD COLUMN dossier_at TIMESTAMPTZ;

ALTER TABLE leads ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();
