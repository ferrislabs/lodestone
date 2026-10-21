CREATE TABLE organisation_settings (
    key        TEXT        PRIMARY KEY CHECK (key <> ''),
    value      TEXT        NOT NULL,
    updated_by UUID        REFERENCES users (id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
