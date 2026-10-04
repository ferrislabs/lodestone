# Glossary

| Term | Definition | Do not say |
|------|------------|------------|
| profile | A market definition stored in Postgres: what to search, how to filter, how to score, how to prompt. | vertical, campaign, config |
| profile revision | An immutable version of a profile. Every edit creates one. | profile version |
| active profile | A profile whose latest revision is scheduled by workers. | enabled profile |
| normalized domain | The registrable domain of a company, lowercase, IDNA-encoded, public suffix aware. Lead key with the profile. | root domain, apex |
| lead | A company, identified by profile and normalized domain, that may buy the profile's product. | prospect, account |
| lead status | One of new, uncertain, qualified, validated, rejected, contacted. A lead whose LLM evidence cannot be verified twice becomes uncertain and never reaches validation. | stage |
| signal | One observed fact about a lead, with source, type, raw data, date, provenance URL and strength. | event, hit |
| source | An adapter that fetches external data: CT, HTTP probe, DNS, BOAMP, DECP, TED, France Travail, ATS. | connector, provider |
| run | One execution of a source for one profile, with start, end, volume and errors. | crawl, batch |
| job | A unit of background work in the Postgres queue, claimed by a worker under a lease. | task |
| lease | The time a worker holds a job before another worker may reclaim it. | lock |
| fingerprint | A profile-defined HTTP check: path, matchers, product class, version extraction, debt markers. | signature |
| product class | The product a fingerprint identifies, defined by the profile. | technology |
| debt marker | A fingerprint result that shows outdated maintenance, such as an old version or a legacy path. | |
| provenance | The URL and method that produced a piece of data. Stored with every signal and contact. | origin |
| contact email | An address attached to a lead, with provenance, method, confidence and kind (role or nominative). | email |
| suppression list | Domains and addresses never to contact, across all profiles. | blacklist, blocklist |
| score | Weighted sum of a lead's signals using its profile's factors and weights. | rating |
| triage | Cheap LLM tier that decides relevance of ambiguous items. | pre-filter |
| qualification | Strong LLM tier that writes an assessment for leads above the profile threshold. | enrichment |
| assessment | LLM output for a lead: summary, approach angle, quoted evidence, draft, model, prompt version, profile revision. | analysis, review |
| evidence | A verbatim excerpt of stored source content that an assessment cites. | proof |
| draft | A message proposed by qualification, waiting for human approval. Never sent by Lodestone. | email, message |
| LLM provider | A named endpoint with kind, base URL, model and encrypted API key, stored in Postgres. | model, backend |
| output | An adapter that pushes leads out: JSON, CSV, webhook, later a CRM. | export target, sink |
