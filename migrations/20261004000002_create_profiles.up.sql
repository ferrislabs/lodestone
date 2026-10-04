CREATE TABLE profiles (
    id         UUID        PRIMARY KEY,
    name       TEXT        NOT NULL UNIQUE,
    active     BOOLEAN     NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX profiles_active_idx ON profiles (id) WHERE active;

CREATE TABLE profile_revisions (
    profile_id UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    revision   INTEGER     NOT NULL CHECK (revision > 0),
    definition JSONB       NOT NULL,
    created_by UUID        REFERENCES users (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (profile_id, revision)
);

CREATE FUNCTION profile_revisions_are_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'profile revisions are immutable';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER profile_revisions_no_update
    BEFORE UPDATE ON profile_revisions
    FOR EACH ROW EXECUTE FUNCTION profile_revisions_are_immutable();
