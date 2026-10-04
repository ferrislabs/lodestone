CREATE INDEX idx_organizations_deleted_at ON organizations (deleted_at);

DROP INDEX IF EXISTS idx_organizations_owner_id;

ALTER TABLE roles
    DROP CONSTRAINT IF EXISTS chk_roles_permissions_non_negative;
