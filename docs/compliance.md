# Compliance notes

What Lodestone does and does not do with other people's servers and data. Every statement cites the code or document that enforces it. This is a description of behaviour, not legal advice. The operator is responsible for the lawfulness of the use made of the data, in particular the first contact with a person.

## No automatic sending

Lodestone never sends a message to a lead.

- A qualified lead gets a draft in status `pending`. A person approves, edits or rejects it in the Drafts queue or through `/api/v1/drafts/*` (`libs/core/src/application/drafts/mod.rs`).
- Approving marks the draft approved and moves the lead to `validated`. Nothing is sent. Moving a lead to `contacted` is a manual status change by a person (`PATCH /api/v1/leads/{id}/status`).
- The workspace has no mail-sending dependency. `apps/api/tests/no_outbound_mail.rs` fails if a manifest declares a known mail-sending crate (`lettre`, `mail-send`, `async-smtp`, SES, SendGrid and others) or if a Rust source file refers to SMTP or sendmail.
- A `security.txt` contact is a hint and can never be a recipient (`libs/core/src/domain/contact/recipient.rs`).
- The only outbound delivery of lead data is the optional signed webhook to a URL the operator configures ([webhooks](webhooks.md)).
- The plan lists what is never done: send an email automatically, scrape a social network or a job board, hard-code market criteria, use an unchecked endpoint, scan for vulnerabilities or access anything non-public (`docs/plan.md`, section "Never").

## Politeness towards third-party servers

All outbound HTTP of the sources goes through one fetcher, `PoliteFetcher` (`libs/fetch/src/fetcher.rs`).

| Rule | Behaviour | Code |
|---|---|---|
| Identifiable User-Agent | `Lodestone/<version> (+<contact>)`. `OUTBOUND_CONTACT` is required at start and must be an email address or an `https://` URL. | `libs/fetch/src/fetcher.rs`, `libs/common/src/settings.rs` |
| Rate limit per host | At most `OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE` requests (default 6) to one host per minute. The limiter is in memory, so the limit applies per fetcher instance and per process. With several replicas the effective rate is multiplied. A refused request fails with `RateLimited`; sources skip the host and retry on a later run. Contact and ATS scans wait up to 75 s, three attempts at most. | `libs/fetch/src/fetcher.rs`, `libs/rate-limit` |
| Higher limits for documented APIs | BOAMP and TED use at least 30 requests per minute per host, DECP at least 120, France Travail 180. | `apps/api/src/main.rs` |
| robots.txt for crawls | Requests with purpose `Crawl` fetch `/robots.txt` of the origin and refuse a disallowed path. The agent token is `Lodestone`. Answer cached 24 hours. A 4xx on robots.txt allows everything. A 5xx, a network error or an unparsable file disallows everything for 10 minutes. | `libs/fetch/src/robots.rs`, `libs/fetch/src/fetcher.rs` |
| Where crawls apply | Public contact pages (`CONTACT_PAGE_PATHS`) and the careers pages used to find an ATS board. | `libs/sources/src/contact_pages.rs`, `libs/sources/src/ats.rs` |
| Documented APIs skip robots.txt | Requests with purpose `Api` do not read robots.txt. Rate limit, public-target check, redirect policy, body cap and User-Agent still apply. Only the sources listed in ADR 0004 may use it: BOAMP, TED, CT monitoring endpoints, DECP, France Travail, and the Greenhouse, Lever and Ashby job board APIs. Adding one requires amending the ADR. | [ADR 0004](adr/0004-documented-apis-skip-robots.md) |
| Probes | The HTTP probe (`Probe`) and the `security.txt` fetch do not read robots.txt. They run only on candidate hosts, request only the paths written in active profiles (or `/.well-known/security.txt` and `/security.txt`), use GET over HTTPS and revisit a host at most once per `OUTBOUND_REVISIT_INTERVAL_HOURS` (default 720). | `libs/sources/src/http_probe.rs`, `libs/sources/src/security_txt.rs` |
| Non-public targets refused | Before each request, and after each redirect, the host is resolved and the request is refused if any address is loopback, private, link-local, unspecified, multicast, documentation, shared (100.64.0.0/10), benchmark (198.18.0.0/15) or reserved, or an IPv6 unique-local or link-local address. Production code leaves `allow_private_targets` false. | `libs/fetch/src/address.rs`, `libs/fetch/src/settings.rs` |
| Redirects | Followed only inside the same registrable domain, at most 5 hops, never for POST. | `libs/fetch/src/fetcher.rs` |
| Body size and time | Body capped (2 MiB by default, 16 MiB for API sources, `DECP_MAX_BODY_MB` for DECP). Timeout `OUTBOUND_REQUEST_TIMEOUT_SECONDS`. | `libs/fetch/src/settings.rs`, `apps/api/src/main.rs` |

Sources and what they touch:

- CT: reads public Certificate Transparency logs through the log operators' monitoring endpoints and keeps only hostnames matching profile labels (ADR [0003](adr/0003-read-ct-from-tiled-logs.md)).
- DNS: CNAME and MX lookups through the system resolver for candidate hosts.
- Tenders and awards: BOAMP, TED, DECP open data (ADR 0004).
- Jobs: the France Travail API, with credentials registered by the operator, and the public job board APIs of companies already detected. The `urlOrigine` of a France Travail offer (often a job board) is never fetched. Domains of job boards, procurement platforms and webmail providers are refused as lead domains (`libs/sources/src/buyer_domain.rs`).
- No social network, no job board scraping, no search engine.

Known limits: the public-target check resolves the name separately from the connection that follows, so it is not a defence against DNS rebinding. The per-host limit is not shared between processes.

## Data minimization

What is stored, by table (`migrations/`):

| Data | Content | Notes |
|---|---|---|
| `candidate_hosts` | Hostnames from CT or an import, with origin and dates. | |
| `signals` | Kind, value, source, strength, date, raw data, provenance URL. | Raw data is small structured JSON. For a probe: host, fingerprint id, path, status, extracted version. For a keyword source: title, matched keywords, route, source details such as buyer name, deadline, offer id, company name. Page bodies and full descriptions are not stored. |
| `contact_emails` | Address, method, confidence, provenance URL, kind. | No name or phone number column. |
| `assessments` | Summary, approach angle, evidence quotes with provenance, draft, model, prompt version, profile revision, review status. | Written by the LLM from the lead's signals. |
| `llm_cache` | Structured LLM output by profile, revision and domain. | |
| `partners`, `partner_awards` | Holder SIRET, name from the public company directory, contract id, buyer, object, amount, date. | From DECP. |
| `suppression_list`, `runs`, `jobs`, `users` | | Users hold the OIDC subject, name, username and email of people who sign in. |

Contacts:

- A public-page or LLM-found address is kept only if its local part is in the profile's `contact_ranking.preferred_local_parts`, and only if it belongs to the lead's own domain or a subdomain (`libs/core/src/domain/contact/mod.rs`). A guessed address needs the local part in `guess_local_parts` and an MX record, and is dropped when a public address exists.
- Profiles are expected to list role mailboxes such as `security` or `contact`. The code does not check that a listed local part is a role and not a person, and every stored contact gets kind `role`. Do not list personal names.
- A `security.txt` `Contact:` address is stored as a hint if it belongs to the lead's domain, whatever its local part. It is never a recipient.

Provenance:

- Every signal stores a provenance URL (`https://...` or `dns:<host>?type=...`). Every contact stores the URL or DNS query that produced it. Every evidence quote stores the provenance of the signal it came from (`libs/core/src/application/qualification/mod.rs`). Evidence must be a verbatim excerpt of stored content, or the assessment is discarded and, after one retry, the lead becomes `uncertain` and never reaches the review queue.

## Suppression list

The suppression list holds domains and email addresses never to be contacted, across all profiles (`suppression_list`: kind `domain` or `email`, unique per value). A domain entry covers the lead domain and every address under it. An email entry matches that exact address.

Where it is enforced:

| Stage | Behaviour | Code |
|---|---|---|
| Candidate hosts | Suppressed domains are not inserted and are not returned for probing or DNS checks. | `libs/core/src/infrastructure/candidate_host/postgres/hosts.rs` |
| ATS targets | Suppressed domains are not scanned. | `libs/core/src/infrastructure/contact_target/postgres/ats.rs` |
| Ingestion | An observation on a suppressed domain creates no lead and no signal. | `record_observations` in `libs/core/src/application/sources/mod.rs` |
| Contact finding | No contact is stored for a suppressed domain or a suppressed address. | `record_contacts` in `libs/core/src/application/contacts/mod.rs` |
| LLM | A suppressed domain is not sent to triage or to qualification. An address returned by triage is dropped if suppressed. | `libs/core/src/application/triage/mod.rs`, `libs/core/src/application/qualification/mod.rs` |
| Review queue | Drafts of suppressed leads are hidden and answer 404. The shown contact skips suppressed addresses. | `libs/core/src/application/drafts/mod.rs` |
| Lead views and status | Lead detail and status change answer 404 for a suppressed domain. Lists exclude them. | `libs/core/src/application/leads/mod.rs`, `libs/core/src/application/output/mod.rs` |
| Exports | Suppressed leads and addresses are excluded from JSON and CSV exports. | `libs/core/src/application/output/mod.rs`, [webhooks](webhooks.md) |
| Webhooks | Checked again when the delivery job runs. A suppressed lead is never sent. | `libs/core/src/application/output/mod.rs`, `apps/api/src/webhook.rs` |

Managing the list: the use cases `add_suppression`, `remove_suppression` and `list_suppressions` exist in `libs/core/src/application/suppression/mod.rs`, but no HTTP route and no screen exposes them. Until one does, edit the table directly:

```sql
INSERT INTO suppression_list (id, kind, value, reason, created_at)
VALUES (gen_random_uuid(), 'domain', 'acme.com', 'asked to be removed', now());

INSERT INTO suppression_list (id, kind, value, reason, created_at)
VALUES (gen_random_uuid(), 'email', 'info@acme.com', 'asked to be removed', now());
```

A domain value must be the normalized domain (registrable domain, lowercase, see the [glossary](glossary.md)). An email value must be lowercase. Adding an entry hides and stops processing what exists; it does not delete the stored rows.

## LLM data handling

- What leaves the system: to the triage provider, the profile's market context (`identity.offer`, `identity.target_customer`, `llm.relevance_criteria`) and one candidate's title and text from a tender, an award or a job offer. To the qualification provider, the market context and the lead's stored signals rendered as text (kind, value, source, date, raw data). No credential and no page body is sent (`libs/core/src/domain/prompt/mod.rs`, `libs/core/src/application/qualification/mod.rs`). Content comes from public sources; keep personal data out of profile text.
- Provider choice: the operator configures providers (`anthropic`, `mistral`, `openai_compatible`) and assigns one to each tier (`triage`, `qualification`). Without an assigned provider no call is made.
- Keys: provider API keys are write-only through the API and encrypted at rest with AES-256-GCM using `LLM_SECRETS_ENCRYPTION_KEY` (`libs/core/src/infrastructure/crypto/`). The key comes from a Kubernetes Secret in the chart.
- Budget: `LLM_RUN_BUDGET_CENTS` (default 500) caps the spend of one run per profile, priced with the provider's per-million-token prices. A provider with zero prices is never charged against the budget. When the budget is spent the run stops and is recorded as failed with `llm budget exhausted`.
- Content is treated as data: prompts forbid following instructions found in documents or in the market context, and the profile context cannot close its own block (`libs/core/prompts/`, `libs/core/src/domain/prompt/mod.rs`).
- Output is checked: structured JSON only, evidence must be verbatim, the model cannot send anything.
- A human reads every draft before any status change to `validated`.

## Retention

Not implemented. No job deletes or expires leads, signals, contacts, assessments, runs, cache entries or candidate hosts, and no API route deletes a lead. Data stays until the operator removes it from Postgres. Deleting a profile cascades to its leads and cache by foreign key, but no route deletes a profile either. Define a retention rule outside Lodestone until one exists.

## Not covered

- Legal basis, information duties and the rules of the country where messages are sent. These belong to the operator.
- Retention schedules and deletion on request, other than the manual SQL above.
- Terms of use of each upstream API beyond ADR 0004. Check them before enabling a source.
- Data processing agreements with the LLM providers.
