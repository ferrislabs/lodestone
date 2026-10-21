ALTER TABLE decision_labels
    DROP CONSTRAINT decision_labels_rejection_note_length,
    DROP CONSTRAINT decision_labels_rejection_note_required,
    DROP CONSTRAINT decision_labels_rejection_reason_kind,
    DROP CONSTRAINT decision_labels_rejection_reason_check,
    DROP COLUMN rejection_note,
    DROP COLUMN rejection_reason;

ALTER TABLE assessments
    DROP COLUMN samples,
    DROP COLUMN generation,
    DROP COLUMN provider_name,
    DROP COLUMN provider_id;

ALTER TABLE qualification_results
    DROP COLUMN samples,
    DROP COLUMN generation,
    DROP COLUMN provider_name;

UPDATE qualification_results
SET prompt_version = substring(prompt_version FROM position('@' IN prompt_version) + 1)
WHERE position('@' IN prompt_version) > 0;

ALTER TABLE qualification_results
    RENAME COLUMN prompt_version TO prompt_hash;
