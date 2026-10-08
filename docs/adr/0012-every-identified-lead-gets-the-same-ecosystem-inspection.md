# 0012. Every identified lead gets the same ecosystem inspection

Status: accepted
Date: 2026-10-08

## Context

ADR 0011 gave each profile an `enrichment` section with two steps, the HTTP probe and the company job offers, each switched on per profile, with trade-specific defaults (the probe looked at `auth`, `sso`, `login`, `id`, `idp`, `accounts` on every lead, which only makes sense for identity products). What a profile learnt about a company depended on how it was configured, the same company was read again for every profile, and the operator could not see what the enrichment had found, only that a step had run.

The user decided (issue 210) that every lead is inspected as much as possible, the same way for every profile, and that the qualification agents of each profile decide what matters.

## Decision

### One inspection, made of modules

An **identified lead** is a lead with a registrable domain, which every lead has. Each one gets the **ecosystem inspection**: a fixed list of modules, the same for every profile.

- Identité officielle (`official_identity`), Vie légale (`legal_life`), Marchés publics (`public_contracts`), Site et stack technique (`website_stack`), Écosystème DNS et e-mail (`dns_email`), Activité de recrutement (`hiring_activity`), Activité récente (`recent_activity`).
- A module is a `ModuleRunner` (core trait, static dispatch through the `EcosystemRunner` enum of the sources crate, no `dyn`) that turns a domain into **findings**: module, key, kind, French label, value, source URL (a confirmed link or the API URL it was read from), observation time and details.
- This decision builds the framework. Its first modules wrap the existing pieces: the company dossier is « Identité officielle », the HTTP probe with the profile fingerprints is part of « Site et stack technique » (with the home page server, technology and generator), the career page and recruiting board reader is « Activité de recrutement ». The other modules come with issues 211 and 212; until then they are listed as « À venir » and have no status (see "Official modules" below for issue 211).

### Shared per domain

Findings and module statuses belong to the registrable domain, not to a lead: two profiles with a lead on the same domain share one inspection. `lead_ecosystem (domain, module, finding_key)` holds the findings, `lead_ecosystem_modules (domain, module)` the status (`pending`, `done`, `not_applicable`, `failed`), the last run, the next run and the error, and `ecosystem_module_runs` one row per pass of a module (leads inspected, findings, latest example), kept 30 days, for the seven-day statistics of the « Enrichissement » page.

### Hosting platforms

A domain under a hosting platform (`github.io`, `vercel.app`, `notion.site`…) is not the site of a company: it is flagged « Site hébergé sur une plateforme » and the modules that read the site are `not_applicable`, so no page is guessed on it. The company research listener skips such domains too: their dossier gets the terminal state `hosting_platform` without any request, so the dossier wait is released at once, and a startup step switches the existing dossiers of these domains to that state (idempotent). The list is data in code (`HOSTING_PLATFORMS`), not a market criterion, and is covered by a test.

### What stays in a profile

A profile keeps only what is specific to it, without any trade default:

- `inspection.enabled` (default `true`), a master switch;
- `enrichment.custom` (« Avancé »): its own fingerprints, excluded products and extra subdomains (empty by default);
- `enrichment.highlight_keywords` (« Mots-clés à mettre en avant »): the job keywords that highlight an offer and make it a `job_offer` signal.

Profile schema 18 upgrades schema 17 when read (`docs/profiles.md`, "Version 17 to 18"): the exact former default subdomains are dropped unless the profile has identity fingerprints.

### Runtime

One continuous listener, `ecosystem_inspection` (ADR 0006), picks the due domains of the active profiles whose inspection is on, new ones first, then those whose module refresh interval passed, and runs every available module through the shared polite fetcher, so per-host rate limits, robots.txt and the confirmed-link checks still apply. The profile-scoped signals keep their producers: the HTTP probe listener for profiles with their own fingerprints and the scheduled `ats` job for highlighted keywords; `lead_enrichments` stays as their internal bookkeeping only.

### Qualification and agents

Qualification keeps the wait of ADR 0011 (`ENRICHMENT_WAIT_MINUTES`): a lead is due once every available module has settled for its domain (any status but `pending`), or once the wait is over, or at once when the inspection of its profile is off; a lead whose only evidence is a certificate still waits for other evidence. The qualification, scoring and aggregator agents receive one « Ecosystem » document per module and source; evidence may quote them.

### Official modules (issue 211)

« Identité officielle », « Vie légale » and « Marchés publics » need the SIREN of the company. It comes from the company dossier, else from the SIREN finding of « Identité officielle » (which runs first), else, when neither the dossier nor the identity module has settled yet, from a company research done by the module itself. Without a SIREN the module is `not_applicable` and its status carries the reason « SIREN inconnu » (stored in the `error` column, no migration).

- « Identité officielle » gives the legal name, SIREN, NAF code and label, headcount range, head office and creation date. The SIREN finding points to the page of the site where it was read (the legal notice), the others to the Recherche d'entreprises API URL.
- « Vie légale » reads the BODACC open data (DILA, Licence Ouverte / Open Licence, no account) on the OpenDataSoft API, dataset `annonces-commerciales`, `records?where=registre="<SIREN>"&order_by=dateparution desc&limit=20`. One finding per announcement: type (création, immatriculation, modification, vente ou cession, radiation, procédure collective with the nature of the judgment, conciliation, rétablissement professionnel, dépôt des comptes), publication date, court, short description and the link to the announcement on bodacc.fr. Refreshed every 7 days. The endpoint is the `base_url` of the new catalog provider `bodacc` (category Entreprises, no authentication, test = GET of the dataset metadata).
- « Marchés publics » reads the consolidated DECP of data.gouv.fr (Licence Ouverte v2.0) through its tabular API, `titulaire_id__contains=<SIREN>`, and keeps the rows whose holder SIRET (or SIREN) belongs to the company: buyer, object, amount, notification date and procedure, the 20 most recent. Refreshed every 14 days. The endpoint is the `holders_url` setting of the `decp` provider.
- A registry error is a `failed` status retried after six hours; a rate limit pauses the pass as for the other modules.
- The three modules respect the catalog readiness of ADR 0010: « Identité officielle » needs `company_research`, « Vie légale » `bodacc` and « Marchés publics » `decp`. While its provider is not ready a module is skipped for the pass (its status is unchanged) and leaves the set of modules the qualification waits for, like a module to come: a lead qualifies as soon as the other available modules settled, and once the provider is ready the module runs and its findings are read at the next qualification, the module card and the lead section show « Fournisseur non prêt » with a link to the provider page of the catalog, and the dry run answers `409` with that message. Missing buyer names are not resolved.
- The agent documents carry one concise line per finding: label, date, value, then the buyer, amount, procedure or court when known.

### Operator

The « Enrichissement » page of a profile shows one card per module (title as a question, what it learns, status, seven-day statistics, « Tester sur un lead »), the master switch and « Avancé » for the profile fingerprints. « Tester sur un lead » calls `POST /api/v1/leads/{id}/ecosystem/dry-run?module=…` (users only), which runs one module now and stores nothing. The lead page has a section « Écosystème » with the findings of each module, their sources, the module statuses and the hosting platform notice.

## Consequences

- Every profile benefits from everything Lodestone can learn about a company, and a new module needs no profile change.
- A company with leads in several profiles is inspected once.
- The website module and the HTTP probe listener may both request the fingerprint paths of a lead: the first records findings for every profile, the second the signals of one profile. Both go through polite fetchers and their revisit intervals.
- Profiles lose their per-step switches: an operator can only switch the whole inspection off, or remove the profile fingerprints and keywords.
- A new finding does not requalify a lead by itself; the inspection runs before the first qualification, and later findings are read at the next one.
