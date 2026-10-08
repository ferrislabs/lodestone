ALTER TABLE llm_providers
    ADD COLUMN max_concurrency SMALLINT NOT NULL DEFAULT 4
        CHECK (max_concurrency BETWEEN 1 AND 32);

UPDATE llm_providers
SET max_concurrency = 2
WHERE kind = 'openai_compatible'
  AND base_url ~* '^https?://(localhost|127\.0\.0\.1|\[::1\])([:/]|$)';
