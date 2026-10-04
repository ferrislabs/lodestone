DROP INDEX IF EXISTS idx_member_roles_organization_id;

ALTER TABLE member_roles
    DROP CONSTRAINT IF EXISTS fk_member_roles_member,
    DROP CONSTRAINT IF EXISTS fk_member_roles_role;

ALTER TABLE member_roles
    ADD CONSTRAINT member_roles_member_id_fkey
        FOREIGN KEY (member_id) REFERENCES organization_members (id) ON DELETE CASCADE,
    ADD CONSTRAINT member_roles_role_id_fkey
        FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE CASCADE;

ALTER TABLE member_roles
    DROP COLUMN organization_id;

ALTER TABLE roles
    DROP CONSTRAINT IF EXISTS uq_roles_id_organization;

ALTER TABLE organization_members
    DROP CONSTRAINT IF EXISTS uq_members_id_organization;
