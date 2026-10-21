ALTER TABLE company_dossiers
    ADD COLUMN ignored_links INTEGER NOT NULL DEFAULT 0 CHECK (ignored_links >= 0),
    ADD COLUMN pages_checked_at TIMESTAMPTZ;

CREATE INDEX company_dossiers_unchecked_pages_idx ON company_dossiers (domain)
    WHERE pages_checked_at IS NULL;
