UPDATE company_dossiers SET state = 'pending', next_attempt_at = now() WHERE state = 'hosting_platform';
ALTER TABLE company_dossiers DROP CONSTRAINT company_dossiers_state_check;
ALTER TABLE company_dossiers
    ADD CONSTRAINT company_dossiers_state_check
        CHECK (state IN ('pending', 'complete', 'partial', 'unreachable'));
DROP TABLE ecosystem_module_runs;
DROP TABLE lead_ecosystem_modules;
DROP TABLE lead_ecosystem;
