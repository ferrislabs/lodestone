CREATE TABLE companies (
    siren      TEXT        PRIMARY KEY CHECK (siren ~ '^[0-9]{9}$'),
    legal_name TEXT        NOT NULL CHECK (legal_name <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE company_dossiers
    ADD COLUMN company_siren TEXT REFERENCES companies (siren);

CREATE INDEX company_dossiers_company_siren_idx
    ON company_dossiers (company_siren)
    WHERE company_siren IS NOT NULL;

INSERT INTO companies (siren, legal_name)
SELECT DISTINCT ON (official->>'siren') official->>'siren', official->>'legal_name'
FROM company_dossiers
WHERE official IS NOT NULL
  AND official->>'siren' ~ '^[0-9]{9}$'
  AND COALESCE(official->>'legal_name', '') <> ''
ORDER BY official->>'siren', built_at DESC NULLS LAST, domain;

UPDATE company_dossiers
SET company_siren = official->>'siren'
WHERE official IS NOT NULL
  AND official->>'siren' IN (SELECT siren FROM companies);

ALTER TABLE lead_qualification_state ADD COLUMN scored_at TIMESTAMPTZ;

UPDATE lead_qualification_state SET scored_at = completed_at;

CREATE VIEW lead_siblings AS
SELECT lead.id AS lead_id, sibling.id AS sibling_id
FROM leads lead
JOIN company_dossiers lead_dossier ON lead_dossier.domain = lead.domain
JOIN company_dossiers sibling_dossier
  ON sibling_dossier.company_siren = lead_dossier.company_siren
JOIN leads sibling
  ON sibling.domain = sibling_dossier.domain
 AND sibling.profile_id = lead.profile_id
 AND sibling.id <> lead.id
WHERE lead_dossier.company_siren IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM suppression_list sup
      WHERE sup.kind = 'domain'
        AND (sup.value = sibling.domain
             OR right(sibling.domain, length(sup.value) + 1) = '.' || sup.value)
  );
