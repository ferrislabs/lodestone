ALTER TABLE assessments
    DROP COLUMN quality_warnings;

ALTER TABLE llm_providers
    DROP COLUMN repeat_penalty,
    DROP COLUMN presence_penalty,
    DROP COLUMN frequency_penalty,
    DROP COLUMN temperature,
    DROP COLUMN max_tokens;
