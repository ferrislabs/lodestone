ALTER TABLE llm_providers
    ADD COLUMN max_tokens        INTEGER          CHECK (max_tokens >= 1 AND max_tokens <= 200000),
    ADD COLUMN temperature       DOUBLE PRECISION CHECK (temperature >= 0 AND temperature <= 2),
    ADD COLUMN frequency_penalty DOUBLE PRECISION CHECK (frequency_penalty >= -2 AND frequency_penalty <= 2),
    ADD COLUMN presence_penalty  DOUBLE PRECISION CHECK (presence_penalty >= -2 AND presence_penalty <= 2),
    ADD COLUMN repeat_penalty    DOUBLE PRECISION CHECK (repeat_penalty >= 1 AND repeat_penalty <= 2);

ALTER TABLE assessments
    ADD COLUMN quality_warnings TEXT[] NOT NULL DEFAULT '{}';
