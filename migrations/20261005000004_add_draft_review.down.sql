DROP INDEX assessments_pending_drafts_idx;

ALTER TABLE assessments
    DROP CONSTRAINT assessments_draft_original_complete,
    DROP CONSTRAINT assessments_draft_decision_recorded,
    DROP CONSTRAINT assessments_draft_review_requires_draft,
    DROP CONSTRAINT assessments_draft_status_matches_draft,
    DROP CONSTRAINT assessments_draft_status_check,
    DROP COLUMN draft_original_body,
    DROP COLUMN draft_original_subject,
    DROP COLUMN draft_rejection_reason,
    DROP COLUMN draft_edited_at,
    DROP COLUMN draft_reviewed_at,
    DROP COLUMN draft_reviewed_by,
    DROP COLUMN draft_status;
