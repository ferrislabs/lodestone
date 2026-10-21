ALTER TABLE lead_qualification_state
    DROP COLUMN summary_error,
    DROP COLUMN summary_next_attempt_at,
    DROP COLUMN summary_attempts;

ALTER TABLE assessments
    DROP CONSTRAINT assessments_draft_suggestion_check,
    DROP COLUMN draft_outdated_at,
    DROP COLUMN draft_suggested_body,
    DROP COLUMN draft_suggested_subject;
