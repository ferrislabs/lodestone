-- Reverting requires the data to satisfy the old constraints again: rows with a
-- NULL email, or duplicate emails or usernames, will make this fail. That is
-- deliberate - silently dropping or mangling them would be worse.
ALTER TABLE users
    ALTER COLUMN email SET NOT NULL;

ALTER TABLE users
    ADD CONSTRAINT users_email_key UNIQUE (email),
    ADD CONSTRAINT users_username_key UNIQUE (username);
