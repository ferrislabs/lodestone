-- The identity provider's subject is the only stable, unique identifier for a
-- user. Email and username are profile attributes: an identity provider may
-- release neither, may release the same value for two people, and may reassign
-- an address when someone leaves. Treating them as keys made provisioning
-- collapse distinct identities onto one row, or fail outright on a collision.
--
-- `sub` keeps its unique constraint - it is the key. The rest become ordinary
-- attributes.

ALTER TABLE users
    ALTER COLUMN email DROP NOT NULL;

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS users_email_key,
    DROP CONSTRAINT IF EXISTS users_username_key;
