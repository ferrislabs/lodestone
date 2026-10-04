CREATE TABLE host_visits (
    host            TEXT        PRIMARY KEY,
    last_probed_at  TIMESTAMPTZ NOT NULL
);
