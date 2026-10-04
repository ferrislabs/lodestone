DROP INDEX IF EXISTS organizations_slug_key;

-- Fails if two organizations now share a slug because one of them is deleted.
-- Reverting has to surface that rather than pick a winner.
ALTER TABLE organizations
    ADD CONSTRAINT organizations_slug_key UNIQUE (slug);
