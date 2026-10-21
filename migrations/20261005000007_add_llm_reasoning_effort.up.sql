ALTER TABLE llm_providers
    ADD COLUMN reasoning_effort TEXT
        CHECK (reasoning_effort IN ('none', 'low', 'medium', 'high'));
