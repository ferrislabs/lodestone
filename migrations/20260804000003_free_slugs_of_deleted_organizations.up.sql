-- The slug uniqueness constraint spanned the whole table, deleted rows
-- included, so a slug stayed reserved forever. Recreating an organization the
-- caller had deleted failed with "slug already taken" - about an organization
-- no read path can show them - and the message doubled as an oracle for the
-- slugs of other tenants' deleted organizations.
--
-- A partial index scopes uniqueness to the rows that are actually live, which
-- is what the rule always meant.

ALTER TABLE organizations
    DROP CONSTRAINT IF EXISTS organizations_slug_key;

CREATE UNIQUE INDEX organizations_slug_key
    ON organizations (slug)
    WHERE deleted_at IS NULL;
