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
- This decision builds the framework. Its first modules wrap the existing pieces: the company dossier is « Identité officielle », the HTTP probe with the profile fingerprints is part of « Site et stack technique » (with the home page server, technology and generator), the career page and recruiting board reader is « Activité de recrutement ». The other modules came with issues 211 and 212 (see "Official modules" and "Site, DNS, hiring and recent modules" below).

### Shared per domain

Findings and module statuses belong to the registrable domain, not to a lead: two profiles with a lead on the same domain share one inspection. `lead_ecosystem (domain, module, finding_key)` holds the findings, `lead_ecosystem_modules (domain, module)` the status (`pending`, `done`, `not_applicable`, `failed`), the last run, the next run and the error, and `ecosystem_module_runs` one row per pass of a module (leads inspected, findings, latest example), kept 30 days, for the seven-day statistics of the « Enrichissement » page.

### Hosting platforms

A domain under a hosting platform (`github.io`, `vercel.app`, `notion.site`…) is not the site of a company: it is flagged « Site hébergé sur une plateforme » and the modules that read the site are `not_applicable`, so no page is guessed on it. The company research listener skips such domains too: their dossier gets the terminal state `hosting_platform` without any request, and a startup step switches the existing dossiers of these domains to that state (idempotent). The list is data in code (`HOSTING_PLATFORMS`), not a market criterion, and is covered by a test.

### What stays in a profile

A profile keeps only what is specific to it, without any trade default:

- `inspection.enabled` (default `true`), a master switch;
- `enrichment.custom` (« Avancé »): its own fingerprints, excluded products and extra subdomains (empty by default);
- `enrichment.highlight_keywords` (« Mots-clés à mettre en avant »): the job keywords that highlight an offer and make it a `job_offer` signal.

Profile schema 18 upgrades schema 17 when read (`docs/profiles.md`, "Version 17 to 18"): the exact former default subdomains are dropped unless the profile has identity fingerprints.

### Runtime

One continuous listener, `ecosystem_inspection` (ADR 0006), picks the due domains of the active profiles whose inspection is on, new ones first, then those whose module refresh interval passed, and runs every available module through the shared polite fetcher, so per-host rate limits, robots.txt and the confirmed-link checks still apply. The profile-scoped signals keep their producers: the HTTP probe listener for profiles with their own fingerprints and the scheduled `ats` job for highlighted keywords; `lead_enrichments` stays as their internal bookkeeping only.

### Qualification and agents

Qualification first kept the wait of ADR 0011 (`ENRICHMENT_WAIT_MINUTES`); "Progressive evaluation" below replaces it: a lead is qualified at once and requalified as the inspection completes. A lead whose only evidence is a certificate still waits for other evidence. The qualification, scoring and aggregator agents receive one « Ecosystem » document per module and source; evidence may quote them.

### Official modules (issue 211)

« Identité officielle », « Vie légale » and « Marchés publics » need the SIREN of the company. It comes from the company dossier, else from the SIREN finding of « Identité officielle » (which runs first), else, when neither the dossier nor the identity module has settled yet, from a company research done by the module itself. Without a SIREN the module is `not_applicable` and its status carries the reason « SIREN inconnu » (stored in the `error` column, no migration).

- « Identité officielle » gives the legal name, SIREN, NAF code and label, headcount range, head office and creation date. The SIREN finding points to the page of the site where it was read (the legal notice), the others to the Recherche d'entreprises API URL.
- « Vie légale » reads the BODACC open data (DILA, Licence Ouverte / Open Licence, no account) on the OpenDataSoft API, dataset `annonces-commerciales`, `records?where=registre="<SIREN>"&order_by=dateparution desc&limit=20`. One finding per announcement: type (création, immatriculation, modification, vente ou cession, radiation, procédure collective with the nature of the judgment, conciliation, rétablissement professionnel, dépôt des comptes), publication date, court, short description and the link to the announcement on bodacc.fr. Refreshed every 7 days. The endpoint is the `base_url` of the new catalog provider `bodacc` (category Entreprises, no authentication, test = GET of the dataset metadata).
- « Marchés publics » reads the consolidated DECP of data.gouv.fr (Licence Ouverte v2.0) through its tabular API, `titulaire_id__contains=<SIREN>`, and keeps the rows whose holder SIRET (or SIREN) belongs to the company: buyer, object, amount, notification date and procedure, the 20 most recent. Refreshed every 14 days. The endpoint is the `holders_url` setting of the `decp` provider.
- A registry error is a `failed` status retried after six hours; a rate limit pauses the pass as for the other modules.
- The three modules respect the catalog readiness of ADR 0010: « Identité officielle » needs `company_research`, « Vie légale » `bodacc` and « Marchés publics » `decp`. While its provider is not ready a module is skipped for the pass (its status is unchanged) and leaves the set of modules the qualification waits for, like a module to come: a lead qualifies as soon as the other available modules settled, and once the provider is ready the module runs and its findings are read at the next qualification, the module card and the lead section show « Fournisseur non prêt » with a link to the provider page of the catalog, and the dry run answers `409` with that message. Missing buyer names are not resolved.
- The agent documents carry one concise line per finding: label, date, value, then the buyer, amount, procedure or court when known.

### Site, DNS, hiring and recent modules (issue 212)

All seven modules are available. None of the four below needs a catalog provider; the three that read the site are `not_applicable` on a hosting platform, « Écosystème DNS et e-mail » still runs there. The detection tables are generic (not market criteria), so they are data in code with tests.

- « Site et stack technique » reads the confirmed home page and up to two confirmed internal pages (distinct first path segments) and matches `TECH_SIGNATURES` (`libs/sources/src/ecosystem/tech.rs`, about 130 entries): HTTP headers, meta tags (generator), script, link and iframe sources, cookie names and HTML markers. One `technology` finding per product: category (`cms`, `framework`, `hosting_cdn`, `analytics`, `chat`, `crm_marketing`, `ecommerce`, `auth`, `payment`, `other`) as label, name and visible version as value, the evidence and the page it was read on. The raw server, technology header and generator stay as findings only when no signature explains them. The profile fingerprints are unchanged. The agent document of this module keeps up to 24 lines (12 for the others).
- « Écosystème DNS et e-mail » reads the MX hosts (`MX_PROVIDERS`: mailbox providers and security gateways), the SPF record (`SPF_INCLUDES`, and the final `all` policy), the TXT verification records (`TXT_VERIFICATIONS`), the DMARC record of `_dmarc.<domain>` and the CNAME chain of `www`, `mail`, `autodiscover`, `status`, `help`, `support`, `docs` and `app` (`CNAME_PROVIDERS`). The resolver gains `DnsRecords` (MX hosts by preference, TXT strings); a truncated UDP answer is retried over TCP on a public resolver. Every lookup failing makes the module `failed`; a missing record is a finding (« Aucun serveur MX », SPF or DMARC « absent »). Source URLs are `dns:<name>?type=<TYPE>`.
- « Activité de recrutement » lists every offer of the recruiting boards and of the schema.org `JobPosting` blocks of the confirmed career pages, not only keyword matches: a `hiring_summary` finding (count, functions from `FUNCTION_KEYWORDS` on the title then the team, top five locations, remote mentions, contract types from `CONTRACT_KEYWORDS` and the board fields, and the trend against the previous pass), then the career pages, the boards and at most 50 offers (title, URL, location, function, contracts, remote, publication date), highlighted ones first. The trend reads the summary stored at the previous pass (the inspection target now carries the stored findings of a module that `keeps_history`) and keeps the last 12 counts.
- « Activité récente » reads the sitemaps listed in `robots.txt` (else `/sitemap.xml`; an index is followed to three child sitemaps, those named after posts or news first), keeps the URLs modified in the last 90 days, finds the news page from the home page links or `/blog`, `/actualites`, `/news`, reads its articles (title, `<time>` or a French, English or numeric date), prefers the sitemap URLs under a news or blog path, and confirms the ten most recent items before listing them (a rate limit while confirming keeps the items already confirmed). Findings: latest news (title and date), site freshness (days since the latest dated update), pages updated in 90 days, news page, and one finding per confirmed item.
- Agent lines read like « Messagerie: Microsoft 365 (MX) », « CMS: WordPress 6.4 », « Recrutement: 12 offres ouvertes : 7 engineering, 3 sales (+4 depuis le dernier passage) », « Dernière actualité: <titre> (<date>) ».
- The lead section and the dry run render these modules by kind: technology chips grouped by category, the DNS summary, the hiring summary with its trend arrow, the latest items with their dates.

### Operator

The « Enrichissement » page of a profile shows one card per module (title as a question, what it learns, status, seven-day statistics, « Tester sur un lead »), the master switch and « Avancé » for the profile fingerprints. « Tester sur un lead » calls `POST /api/v1/leads/{id}/ecosystem/dry-run?module=…` (users only), which runs one module now and stores nothing. The lead page has a section « Écosystème » with the findings of each module, their sources, the module statuses and the hosting platform notice.

## Consequences

- Every profile benefits from everything Lodestone can learn about a company, and a new module needs no profile change.
- A company with leads in several profiles is inspected once.
- The website module and the HTTP probe listener may both request the fingerprint paths of a lead: the first records findings for every profile, the second the signals of one profile. Both go through polite fetchers and their revisit intervals.
- Profiles lose their per-step switches: an operator can only switch the whole inspection off, or remove the profile fingerprints and keywords.
- ~~A new finding does not requalify a lead by itself.~~ Superseded by "Progressive evaluation": new or changed findings requalify the lead, debounced and capped.

## Amendment: progressive evaluation (issue 217)

Date: 2026-10-08

### Context

Waiting up to 30 minutes for the inspection (`ENRICHMENT_WAIT_MINUTES`) and 10 for the dossier (`COMPANY_RESEARCH_WAIT_MINUTES`) delayed every lead, and a finding that arrived after the first qualification was only read at the next one, which might never come. The user decided (2026-10-08) that there is no imposed wait, and that the status of a lead evolves over time.

### Decision

- **No wait.** Both settings are removed: a lead with evidence is qualified as soon as the qualification listener sees it, with whatever findings and dossier exist. The certificate-only rule stays: a lead whose only evidence is `certificate_seen` (contact signals aside) is never sent to the models. Old `.env` files may still set the two variables; they are ignored.
- **Requalification on new information.** Each module run hashes its findings (key, kind, label, value and details; the observation time, the order and the values relative to the pass, the days of « Fraîcheur du site » and the trend of « Recrutement », are left out) into `lead_ecosystem_modules.content_hash`; a different hash sets `changed_at`. A changed hash, or a new dossier `content_at` (ADR 0007, which replaces the former `dossier_at` comparison of the due query), marks every lead of the domain in `lead_requalifications`: `requalify_after = now + REQUALIFY_DEBOUNCE_SECONDS` (default 120), pushed by each later change, so modules finishing together give one requalification. A first run that finds nothing, a failed run and a module that ran before the hash existed (no stored hash yet) mark nothing.
- **Cap.** A mark is due once `requalify_after` passed, at most `REQUALIFY_MAX_PER_DAY` (default 3) times per lead and UTC day, counted in `daily_count` and `counted_on`; beyond, it waits for midnight UTC. `POST /api/v1/leads/{id}/reevaluate` (users only) sets a forced mark due at once, outside the cap and not counted. It also sets `fresh_calls`: the qualification, scoring and aggregator calls of that lead skip the LLM cache read and store their fresh answers in the cache, until the aggregator summary of that evaluation is stored.
- **Backfill.** A startup step marks, debounced and capped as usual, the leads whose dossier `content_at` is newer than their last evaluation (`leads.qualified_at`), as the former `dossier_at` comparison would have requalified them. A lead already marked is left as is, so the step is idempotent.
- **Consistency.** The evaluation records the time it started reading (`Completion.read_at`); completing it clears the marks set before, and counts the mark when it was a due requalification of an already evaluated lead. A mark set while the agents ran stays and gives another requalification. A requalification runs the qualification agents, then scoring and aggregation, as a signal from a new source does; edited drafts stay protected as before. Rescoring without model calls is unchanged.
- **Status.** The list and the detail expose `evaluation` (stage, `evaluated_at`, modules done among those whose provider is ready, planned requalification time and reason, modules changed since the evaluation). The webapp shows « Non évalué », « Certificat seul — en attente d'une autre preuve », « Réévaluation prévue à HH:MM (nouvelles informations) », « Évalué — inspection en cours (3/7 modules) » and « Évalué — inspection complète », a « Réévaluer » button and the modules changed since the last evaluation, and refreshes while the status can move.
- **Inspection priority.** Each pass of a module first inspects up to `INSPECTION_PRIORITY_BATCH` (default 30) due domains never inspected by it or with a lead created less than an hour ago, newest lead first, then refreshes up to 10 due domains.
- **Country filter.** Certificates and DNS agents take `countries` (schema 19). A ccTLD of a listed country keeps the host, a ccTLD of another country drops it before the lead exists (counted in the run report), a generic TLD keeps it until the dossier gives a head office outside the list; then the lead is suppressed for its profile (`leads.excluded_reason` « hors pays visé ») when all its sourcing signals come from such filtered agents. A later signal of the lead from a sourcing agent whose own filter admits it (or that has no filter) lifts that exclusion; the SaaS-tenant exclusion is never lifted. The IAM example targets France on both agents.
- **SaaS tenants.** `SAAS_TENANT_DOMAINS` (data in code, next to `HOSTING_PLATFORMS`, with a test) lists the tenant suffixes of SaaS providers (Salesforce, Okta, Auth0, Entra, SharePoint, Zendesk, Freshdesk, Atlassian, Shopify, CloudFront, Azure Front Door, Heroku, AWS…). A certificates or DNS host under them never creates a lead; a startup step suppresses the existing leads of active profiles on them with the reason « domaine d'un fournisseur SaaS » (idempotent).
- **Listener errors.** A successful pass clears `listeners.last_error`; `last_error_at` dates the error, and the Sources view shows it only when it is not older than the latest activity.

### Consequences

- A lead shows a verdict within minutes of its first evidence; its verdict may change a few times a day as the inspection fills in.
- A busy domain costs at most three automatic requalifications per lead and day; the operator can always ask for one more.
- Leads suppressed for a profile stay in the database with their reason; they are not deleted.
