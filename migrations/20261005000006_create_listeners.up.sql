CREATE TABLE listeners (
    name             TEXT        PRIMARY KEY,
    holder           TEXT,
    state            TEXT        NOT NULL CHECK (state IN ('standby', 'listening', 'idle', 'idle_no_agent', 'error', 'stopped')),
    last_activity_at TIMESTAMPTZ,
    last_error       TEXT,
    items_total      BIGINT      NOT NULL DEFAULT 0 CHECK (items_total >= 0),
    signals_total    BIGINT      NOT NULL DEFAULT 0 CHECK (signals_total >= 0),
    backlog          BIGINT      CHECK (backlog >= 0),
    updated_at       TIMESTAMPTZ NOT NULL
);

DELETE FROM jobs
WHERE kind = 'source.run'
  AND dedup_key IN (
      'source:certificate_transparency',
      'source:dns_cname',
      'source:http_probe',
      'source:security_txt',
      'source:public_page',
      'source:role_guess'
  );
