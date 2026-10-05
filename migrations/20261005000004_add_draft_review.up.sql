ALTER TABLE assessments
    ADD COLUMN draft_status          TEXT,
    ADD COLUMN draft_reviewed_by     UUID REFERENCES users (id) ON DELETE SET NULL,
    ADD COLUMN draft_reviewed_at     TIMESTAMPTZ,
    ADD COLUMN draft_edited_at       TIMESTAMPTZ,
    ADD COLUMN draft_rejection_reason TEXT,
    ADD COLUMN draft_original_subject TEXT,
    ADD COLUMN draft_original_body    TEXT;

UPDATE assessments SET draft_status = 'pending' WHERE draft_subject IS NOT NULL;

ALTER TABLE assessments
    ADD CONSTRAINT assessments_draft_status_check
        CHECK (draft_status IN ('pending', 'approved', 'rejected')),
    ADD CONSTRAINT assessments_draft_status_matches_draft
        CHECK ((draft_subject IS NULL) = (draft_status IS NULL)),
    ADD CONSTRAINT assessments_draft_review_requires_draft
        CHECK (
            draft_status IS NOT NULL
            OR (draft_reviewed_by IS NULL
                AND draft_reviewed_at IS NULL
                AND draft_edited_at IS NULL
                AND draft_rejection_reason IS NULL
                AND draft_original_subject IS NULL
                AND draft_original_body IS NULL)
        ),
    ADD CONSTRAINT assessments_draft_decision_recorded
        CHECK ((draft_status = 'pending') = (draft_reviewed_at IS NULL) OR draft_status IS NULL),
    ADD CONSTRAINT assessments_draft_original_complete
        CHECK ((draft_original_subject IS NULL) = (draft_original_body IS NULL));

CREATE INDEX assessments_pending_drafts_idx
    ON assessments (lead_id, created_at DESC)
    WHERE draft_status = 'pending';
