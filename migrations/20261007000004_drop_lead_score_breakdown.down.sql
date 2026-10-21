ALTER TABLE leads ADD COLUMN score_breakdown JSONB NOT NULL DEFAULT '[]'::jsonb;
