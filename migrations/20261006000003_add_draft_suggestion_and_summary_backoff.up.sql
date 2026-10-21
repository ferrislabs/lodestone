ALTER TABLE assessments
    ADD COLUMN draft_suggested_subject TEXT,
    ADD COLUMN draft_suggested_body    TEXT,
    ADD COLUMN draft_outdated_at       TIMESTAMPTZ,
    ADD CONSTRAINT assessments_draft_suggestion_check
        CHECK ((draft_suggested_subject IS NULL) = (draft_suggested_body IS NULL));

ALTER TABLE lead_qualification_state
    ADD COLUMN summary_attempts        INTEGER     NOT NULL DEFAULT 0 CHECK (summary_attempts >= 0),
    ADD COLUMN summary_next_attempt_at TIMESTAMPTZ,
    ADD COLUMN summary_error           TEXT;
