DROP VIEW lead_siblings;

ALTER TABLE lead_qualification_state DROP COLUMN scored_at;

DROP INDEX company_dossiers_company_siren_idx;

ALTER TABLE company_dossiers DROP COLUMN company_siren;

DROP TABLE companies;
