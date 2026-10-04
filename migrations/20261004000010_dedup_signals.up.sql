ALTER TABLE signals ADD COLUMN fingerprint TEXT;

DELETE FROM signals a
USING signals b
WHERE a.lead_id = b.lead_id
  AND a.kind = b.kind
  AND COALESCE(a.value, '') = COALESCE(b.value, '')
  AND a.source = b.source
  AND a.provenance = b.provenance
  AND (a.observed_at, a.id) < (b.observed_at, b.id);

UPDATE signals
SET fingerprint = kind || chr(31) || COALESCE(value, '') || chr(31) || source || chr(31) || provenance;

ALTER TABLE signals ALTER COLUMN fingerprint SET NOT NULL;

CREATE UNIQUE INDEX signals_lead_fingerprint_idx ON signals (lead_id, fingerprint);
