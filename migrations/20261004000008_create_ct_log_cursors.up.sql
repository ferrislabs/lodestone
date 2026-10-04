CREATE TABLE ct_log_cursors (
    log_id     TEXT        PRIMARY KEY,
    next_index BIGINT      NOT NULL CHECK (next_index >= 0),
    updated_at TIMESTAMPTZ NOT NULL
);
