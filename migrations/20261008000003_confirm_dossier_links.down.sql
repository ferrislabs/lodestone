DROP INDEX IF EXISTS company_dossiers_unchecked_pages_idx;

ALTER TABLE company_dossiers
    DROP COLUMN IF EXISTS pages_checked_at,
    DROP COLUMN IF EXISTS ignored_links;
