-- `permissions` is a signed BIGINT with no lower bound, so a negative value was
-- storable - and a negative bitfield satisfies every `contains` check, including
-- for permissions that do not exist yet. Nothing should ever write one; the
-- constraint makes sure nothing can.
ALTER TABLE roles
    ADD CONSTRAINT chk_roles_permissions_non_negative CHECK (permissions >= 0);

-- `organizations.owner_id` is a foreign key with no index, so every delete of a
-- user scanned the table.
CREATE INDEX idx_organizations_owner_id ON organizations (owner_id);

-- A full B-tree over a column that is NULL for every live row: it costs a write
-- on every insert and update, and answers no query. The predicate that matters
-- is already served by the partial unique index on the slug.
DROP INDEX IF EXISTS idx_organizations_deleted_at;
