ALTER TABLE qualification_results
    RENAME COLUMN prompt_hash TO prompt_version;

UPDATE qualification_results
SET prompt_version = 'qualification-agent@' || prompt_version
WHERE prompt_version <> '' AND position('@' IN prompt_version) = 0;

ALTER TABLE qualification_results
    ADD COLUMN provider_name TEXT    NOT NULL DEFAULT '',
    ADD COLUMN generation    JSONB   NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN samples       INTEGER NOT NULL DEFAULT 1 CHECK (samples >= 1);

ALTER TABLE assessments
    ADD COLUMN provider_id   UUID,
    ADD COLUMN provider_name TEXT    NOT NULL DEFAULT '',
    ADD COLUMN generation    JSONB   NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN samples       INTEGER NOT NULL DEFAULT 1 CHECK (samples >= 1);

ALTER TABLE decision_labels
    ADD COLUMN rejection_reason TEXT,
    ADD COLUMN rejection_note   TEXT,
    ADD CONSTRAINT decision_labels_rejection_reason_check
        CHECK (rejection_reason IS NULL OR rejection_reason IN (
            'hors_cible', 'deja_client', 'concurrent', 'editeur_integrateur',
            'hebergeur_agence', 'mauvaise_donnee', 'autre'
        )),
    ADD CONSTRAINT decision_labels_rejection_reason_kind
        CHECK (rejection_reason IS NULL OR kind IN ('lead_rejected', 'draft_rejected')),
    ADD CONSTRAINT decision_labels_rejection_note_required
        CHECK (rejection_reason IS DISTINCT FROM 'autre' OR rejection_note IS NOT NULL),
    ADD CONSTRAINT decision_labels_rejection_note_length
        CHECK (rejection_note IS NULL OR (char_length(rejection_note) BETWEEN 1 AND 500 AND rejection_reason IS NOT NULL));
