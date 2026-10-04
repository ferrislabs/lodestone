CREATE TABLE partners (
    id            UUID        PRIMARY KEY,
    profile_id    UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    siret         TEXT        NOT NULL CHECK (siret ~ '^[0-9]{14}$'),
    name          TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL,
    last_award_at DATE,
    award_count   INTEGER     NOT NULL DEFAULT 0 CHECK (award_count >= 0),
    UNIQUE (profile_id, siret)
);

CREATE TABLE partner_awards (
    id          UUID             PRIMARY KEY,
    partner_id  UUID             NOT NULL REFERENCES partners (id) ON DELETE CASCADE,
    contract_id TEXT             NOT NULL,
    buyer_siret TEXT             NOT NULL CHECK (buyer_siret ~ '^[0-9]{14}$'),
    object      TEXT             NOT NULL,
    amount      DOUBLE PRECISION,
    notified_on DATE             NOT NULL,
    provenance  TEXT             NOT NULL,
    matched     JSONB            NOT NULL,
    UNIQUE (partner_id, contract_id)
);
