-- `member_roles` linked a member to a role with no organization of its own, so
-- nothing prevented pairing a member of one organization with a role belonging
-- to another. Carrying the organization on the link row and pointing both
-- foreign keys at composite keys makes that pairing unrepresentable: the
-- database refuses it even if the application forgets to check.

-- Composite keys the link row can reference. Both are already unique on `id`
-- alone; these add the pair so it can be a foreign-key target.
ALTER TABLE organization_members
    ADD CONSTRAINT uq_members_id_organization UNIQUE (id, organization_id);

ALTER TABLE roles
    ADD CONSTRAINT uq_roles_id_organization UNIQUE (id, organization_id);

ALTER TABLE member_roles
    ADD COLUMN organization_id UUID;

-- Backfill from the member side. Any row whose member and role disagree on the
-- organization is a pre-existing inconsistency and will fail the constraint
-- below, which is the intended outcome: it must be seen, not silently kept.
UPDATE member_roles mr
SET organization_id = om.organization_id
FROM organization_members om
WHERE om.id = mr.member_id;

ALTER TABLE member_roles
    ALTER COLUMN organization_id SET NOT NULL;

ALTER TABLE member_roles
    DROP CONSTRAINT IF EXISTS member_roles_member_id_fkey,
    DROP CONSTRAINT IF EXISTS member_roles_role_id_fkey;

ALTER TABLE member_roles
    ADD CONSTRAINT fk_member_roles_member
        FOREIGN KEY (member_id, organization_id)
        REFERENCES organization_members (id, organization_id)
        ON DELETE CASCADE,
    ADD CONSTRAINT fk_member_roles_role
        FOREIGN KEY (role_id, organization_id)
        REFERENCES roles (id, organization_id)
        ON DELETE CASCADE;

CREATE INDEX idx_member_roles_organization_id ON member_roles (organization_id);
