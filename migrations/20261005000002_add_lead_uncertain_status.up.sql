ALTER TABLE leads DROP CONSTRAINT leads_status_check;
ALTER TABLE leads ADD CONSTRAINT leads_status_check
    CHECK (status IN ('new', 'qualified', 'validated', 'rejected', 'contacted', 'uncertain'));
