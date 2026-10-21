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
| unknown option | The option of a question marked `is_unknown`, exactly one per question. A qualification agent picks it when nothing in the documents supports an answer; an answer without a verified quote is downgraded to it. | other, n/a |
| grounded answer | A qualification answer whose `evidence` quote is found verbatim in the documents given to the agent. | sourced answer |
| draft | A message proposed by qualification, waiting for human approval. Never sent by Lodestone. | email, message |
| draft status | One of pending, approved, rejected. Only pending drafts can be edited or decided. Approving marks the draft ready and validates its lead; it never sends anything. | sent, published |
| LLM provider | A named endpoint with kind, base URL, model and encrypted API key, stored in Postgres. | model, backend |
| output | An adapter that pushes leads out: JSON, CSV, webhook, later a CRM. | export target, sink |
| company dossier | What the system knows about the company behind a lead domain, built without a model by the `company_research` listener: pages of the company's own site (home, about, legal notice) and official register data fetched with the SIREN found in the legal notice. One per registrable domain, shared by its leads. State: pending, complete, partial or unreachable. | company card, enrichment |
| SIREN | The 9-digit French company identifier, checked with the Luhn algorithm. Read from a labelled SIREN or SIRET on the company site, never guessed from a name. | |
| decision label | The record of one human decision (lead validated or rejected, draft approved or rejected) with a snapshot of the lead's confidence, evidence, verdict and agent scores at that moment. Written in the decision's transaction, never edited. Basis of the quality metrics and threshold suggestions. | feedback, annotation |
| run provenance | The provider, model, prompt version, generation settings and sample count behind one qualification result or assessment. Stored with the result and copied into the decision label snapshots, so precision can be compared by version. | trace, metadata |
| rejection reason | One of `hors_cible`, `deja_client`, `concurrent`, `editeur_integrateur`, `hebergeur_agence`, `mauvaise_donnee`, `autre`, plus an optional note, required to reject a lead or a draft and stored on the decision label. | cause, motif |
| exclusion | An option marked `excludes` that a qualification agent picked with a verified quote and a probability at or above its `exclusion_threshold`. It sets the verdict to `ecarte` with reason `excluded`, whatever the confidence. | disqualification, blacklist |
| sample | One independent contract call of a qualification agent on a lead. An agent runs `samples` of them (1 to 5) and the answers are voted per axis. | run, attempt |
| agreement | Per axis, the share of the valid samples that chose the final option, stored in the answer and shown as "Accord : 2/3". | consensus, confidence |
| escalation | The re-evaluation of a lead whose confidence falls in the grey band by the profile's escalation provider: one sample at temperature 0 per enabled agent, the results replacing the originals and marked `escalated`. At most once per lead and profile revision. | retry, fallback |
| company entity | One French company identified by its SIREN (`companies`), shared by every lead whose domain's dossier holds that SIREN. Leads stay one per domain and profile; facts and evidence aggregate the siblings of the same profile. Leads without a SIREN stay standalone. | account, organisation |
| sibling lead | Another lead of the same company entity. Only siblings of the same profile count in facts and evidence; those of other profiles are shown on the lead page. | duplicate, child |
| independent evidence | Signals with the same kind, normalised value and source count once in the evidence volume, across the siblings of a company. The count of independent signals is `independent_signals`. | unique signals, deduplicated signals |
| half-life | The age in days at which a signal of a given kind counts for half in the evidence volume and strength (`evidence.half_life_days`, by signal kind, `default` for the others). A signal weighs `0.5 ^ (age / half-life)`. | expiry, TTL |
| scoring agent | An LLM agent that decides the score and the verdict through a normed contract imposed by the application (ADR 0009, implemented by issue 172). Several per profile, each weighted; the score is the weighted mean and the verdict the argmax of the weighted verdict probabilities. | judge, classifier |
| indicators | The values computed in code and given to the scoring agents: the agents' opinion, the evidence strength with its components, independent signals and decayed volume. Shown as "Calcul" on the lead page. | metrics, features |
