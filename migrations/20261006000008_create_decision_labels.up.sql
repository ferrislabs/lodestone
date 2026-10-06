CREATE TABLE decision_labels (
    id               UUID        PRIMARY KEY,
    lead_id          UUID        NOT NULL REFERENCES leads (id) ON DELETE CASCADE,
    profile_id       UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    profile_revision INTEGER     NOT NULL CHECK (profile_revision > 0),
    kind             TEXT        NOT NULL CHECK (kind IN ('lead_validated', 'lead_rejected', 'draft_approved', 'draft_rejected')),
    decided_by       UUID        REFERENCES users (id) ON DELETE SET NULL,
    decided_at       TIMESTAMPTZ NOT NULL,
    snapshot         JSONB       NOT NULL
);

CREATE INDEX decision_labels_profile_decided_idx ON decision_labels (profile_id, decided_at);
CREATE INDEX decision_labels_lead_idx ON decision_labels (lead_id);
