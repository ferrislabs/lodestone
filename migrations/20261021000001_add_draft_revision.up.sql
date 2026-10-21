ALTER TABLE assessments
    ADD COLUMN draft_revision INTEGER NOT NULL DEFAULT 0 CHECK (draft_revision >= 0);
