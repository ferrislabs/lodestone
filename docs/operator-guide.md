# Operator guide

For the person who runs Lodestone. Writing profiles: [profile authoring](profile-authoring.md). Every setting: [configuration](configuration.md). Rules about third parties and data: [compliance](compliance.md). Terms: [glossary](glossary.md).

## Architecture

1. One Rust binary (`apps/api`) and one Postgres. No Redis, no broker (ADR 0001, `CLAUDE.md`).
2. The binary runs two HTTP servers (public API on `SERVER_PORT`, internal `/health` and `/metrics` on `SERVER_INTERNAL_PORT`) and a pool of job workers.
3. Background work is a row in the `jobs` table, claimed by a worker under a lease. At start the binary enqueues one recurring job per source and one for qualification (`apps/api/src/jobs.rs`).
4. Sources collect data for the active profiles: Certificate Transparency and imported hostnames feed candidate hosts; the HTTP probe and DNS CNAME check examine them; BOAMP, DECP, TED, France Travail and ATS job boards bring tenders and job offers.
5. Findings become signals on a lead (a company, per profile). The lead score is a weighted sum defined by the profile.
6. Contact finders look for role addresses on the lead's public pages, in `security.txt` and by MX-checked guesses.
7. Triage (cheap LLM tier) judges ambiguous tender and job candidates. Keyword rules decide the clear ones.
8. Qualification (strong LLM tier) writes an assessment and a draft for leads at or above the profile threshold. Evidence must be verbatim.
9. A person approves or rejects drafts in the validation queue. Approving validates the lead. Lodestone never sends a message.
10. Outputs: JSON and CSV export, and an optional signed webhook per validated lead ([webhooks](webhooks.md)).

## Deploy

### Kubernetes

Use the Helm chart: [Helm guide](deploy/helm.md). It covers secrets, the migration hook, probes, replicas and upgrades. In short: the chart deploys the API, the webapp and a migration Job; secrets are referenced by name from existing Kubernetes Secrets and never stored in the chart. Every replica runs job workers, and the queue claims jobs with `FOR UPDATE SKIP LOCKED` and a lease, so replicas do not run the same job twice. The per-host politeness limit is held in memory per process, so it is multiplied by the number of replicas.

### Local development

Commands from `README.md` and `CLAUDE.md`.

```bash
cp .env.example .env              # then fill the required values, see below
docker compose up -d postgres     # Postgres on localhost:5433
export DATABASE_URL=postgres://ferriskey:ferriskey@localhost:5433/lodestone
cargo install sqlx-cli --no-default-features --features rustls,postgres
sqlx migrate run
cargo run -p api                  # public :3456, internal :3457
```

`docker compose up -d` without a service name also starts the SigNoz stack (ClickHouse and others, `SIGNOZ_JWT_SECRET` required in `.env`). Webapp:

```bash
cd apps/webapp
pnpm install
pnpm dev                          # http://localhost:3000
```

The webapp reads `VITE_API_URL`, `VITE_OIDC_AUTHORITY` and `VITE_OIDC_CLIENT_ID` (`apps/webapp/.env.example`).

## First-run checklist

1. **Postgres and migrations.** The API does not migrate by itself. Run `sqlx migrate run` against the database, or let the Helm hook do it. Without it the first query fails.
2. **Required settings.** `DATABASE_NAME`, `AUTH_CLIENT_SECRET`, `OUTBOUND_CONTACT`, `LLM_SECRETS_ENCRYPTION_KEY`. Set `DATABASE_PASSWORD` explicitly (the code has a default, see [configuration](configuration.md)). `OUTBOUND_CONTACT` is an email address or an `https://` URL that third parties can use to reach you: it appears in the User-Agent of every request.
3. **Encryption key.** Generate with `openssl rand -base64 32` and store it in your secret manager. It encrypts LLM provider API keys at rest. If it changes, stored provider keys cannot be decrypted: the tier stops working until you re-enter each key with `PATCH /api/v1/llm/providers/{id}`.
4. **FerrisKey client.** The API checks bearer tokens issued by `AUTH_ISSUER` (and for `AUTH_AUDIENCE` when set) and uses `AUTH_CLIENT_ID` / `AUTH_CLIENT_SECRET` towards the identity provider. The webapp needs its own public OIDC client (`VITE_OIDC_CLIENT_ID`, default `lodestone-webapp`) with the webapp URL as redirect. Writes (profiles, providers, drafts, lead status) need a token of a signed-in user; a service-account token gets 403 on them. Creating the realm and clients is done in FerrisKey and is not described here.
5. **Start the binary.** The logs show `starting api, internal http servers and job workers`, then `scheduled first source run` for each source.
6. **Create LLM providers and assign tiers.** In the UI: **LLM providers** > **New provider**, then **Tier assignment**. Or through the API:

   ```bash
   curl -sS -X POST "$API/llm/providers" -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" -d '{
       "name": "mistral-small", "kind": "mistral",
       "base_url": "<provider API URL>", "model": "<model name>",
       "api_key": "<key>",
       "pricing": {"input_cents_per_mtok": 10, "output_cents_per_mtok": 30}
     }'
   curl -sS -X POST "$API/llm/providers/<id>/test" -H "Authorization: Bearer $TOKEN"
   curl -sS -X PUT  "$API/llm/tiers/triage"        -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" -d '{"provider_id": "<id>"}'
   curl -sS -X PUT  "$API/llm/tiers/qualification" -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" -d '{"provider_id": "<id>"}'
   ```

   Kinds: `anthropic`, `mistral`, `openai_compatible`. `api_key` is required for `anthropic` and `mistral`, write-only, never returned. `max_retries` (0 to 10, default 2) and `timeout_seconds` (1 to 600, default 30) are optional. The prices above are placeholders: use your provider's price per million tokens. With zero prices (the default) the run budget never counts any spend. The test call is one minimal request with no retry and no budget charge. A provider assigned to a tier cannot be deleted. Without an assigned triage provider, ambiguous candidates are skipped. Without a qualification provider, no assessment is written.
7. **Import a profile and activate it.** See [profile authoring](profile-authoring.md#import-and-activate). The profile starts inactive; only active profiles are scheduled.
8. **Give the probe something to look at.** CT fills candidate hosts over time (the first run starts each log at its current end, so nothing earlier is read). To start with a known list, feed hostnames on stdin: `cargo run -p api --bin import_hosts < hosts.txt` (same database settings as the API).
9. **Watch it work.** Logs (`source run finished`, `triage finished`, `qualification run finished`), the `runs` and `jobs` tables (queries below), the Leads and Drafts screens, and metrics: [metrics](metrics.md). On the internal server, `GET /health` returns `ok`.

## Day-2 operations

### Jobs, schedules and retries

Job kinds: `source.run` (one per source, payload `{"source": "<name>"}`), `llm.qualify`, `webhook.deliver`.

| Job | Interval after a run |
|---|---|
| `certificate_transparency` | `CT_INTERVAL_SECONDS` (60 s) |
| `decp` | `DECP_INTERVAL_HOURS` (24 h) |
| `dns_cname`, `http_probe`, `public_page`, `security_txt`, `role_guess`, `boamp`, `ted`, `ats`, `france_travail`, `llm.qualify` | `JOBS_SOURCE_INTERVAL_MINUTES` (60 min) |

`france_travail` is scheduled only if both credentials are set. Jobs have a `dedup_key` (`source:<name>`, `llm:qualify`), so a restart does not duplicate them.

- A worker claims a job with a lease of `JOBS_LEASE_SECONDS` (300 s). If the process dies, another worker takes the job after the lease expires, so handlers must be safe to run twice.
- On error a job is retried with a delay of `JOBS_RETRY_BASE_SECONDS` (10 s) doubling per attempt, capped at one hour.
- Source and qualification jobs get 3 attempts. After the last failed attempt the handler logs it and schedules the next normal run, so these jobs do not stay `failed`. Look at `runs.error` and the logs.
- `JOBS_MAX_ATTEMPTS` (5) sets the attempts of webhook deliveries. After the last attempt a delivery job is `failed`. A `410 Gone` from the receiver ends it at once.
- On shutdown running jobs get `JOBS_SHUTDOWN_GRACE_SECONDS` (20 s), then their lease is released.

```sql
SELECT kind, status, count(*) FROM jobs GROUP BY 1, 2;
SELECT kind, payload->>'source' AS source, attempts, run_at, last_error FROM jobs ORDER BY run_at;
SELECT source, status, started_at, volume, signals, error FROM runs ORDER BY started_at DESC LIMIT 30;
```

### LLM budgets

`LLM_RUN_BUDGET_CENTS` (default 500) caps the spend of one run for one profile, for triage and for qualification separately. It is priced from the provider's per-million-token prices. When it is spent, the run stops, the run row is marked `failed` with `llm budget exhausted`, and the remaining work is picked up by the next run. Cached answers cost nothing; the cache is keyed by profile revision, so a new revision pays again. Qualification handles 20 leads per profile per run, highest score first.

### Suppression list

Domains and addresses never to be contacted, across all profiles. No API route or screen manages it yet; add and remove rows in the `suppression_list` table. SQL, enforcement points and caveats: [compliance](compliance.md#suppression-list).

### New profile revision

Saving a changed definition creates a revision; an identical one is a no-op. A new revision invalidates the LLM cache for that profile, but it does not rescore existing leads (they are rescored when their next signal or contact arrives) and it does not re-queue leads that are already qualified, validated or rejected. Details: [profile authoring](profile-authoring.md#revisions).

### Uncertain leads

A lead becomes `uncertain` when the model's quoted evidence cannot be found verbatim in the lead's signals after one retry. It never reaches the review queue. To try again, move it back to `new`; it is qualified again on the next run. The model's first answer is cached even when its evidence fails the check, so the same first answer comes back from the cache and only the retry call can differ. A new profile revision, or an edited `relevance_criteria`, makes a fresh first call:

```bash
curl -sS -X PATCH "$API/leads/<lead_id>/status" -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" -d '{"status": "new"}'
```

From `uncertain` a lead can also go to `rejected`. In the UI the lead page offers the allowed transitions. `GET /api/v1/profiles/{id}/leads?status=uncertain` lists them.

### DECP download size

The DECP source downloads the consolidated monthly files of data.gouv.fr in full on every run: about 140 MB for a full month. With `DECP_LOOKBACK_DAYS=31` it reads the current and the previous month, so a run can move around 280 MB, once a day by default. A file larger than `DECP_MAX_BODY_MB` (256) is refused. The file is held in memory and parsed from that buffer, so check the memory limit of the API container (the chart default is `1Gi`) against your peak. The request timeout for DECP is at least 900 s. If you do not need awards, do not put `decp` in a profile: the source then does nothing.

### France Travail credentials

The source needs an OAuth2 client registered with France Travail (partner API, ADR 0004). Set `FRANCE_TRAVAIL_CLIENT_ID` and `FRANCE_TRAVAIL_CLIENT_SECRET` (from a Secret in Kubernetes) and restart: the job is scheduled at start only when both are non-empty. Without them the log says `france travail source disabled`, and a profile with `france_travail` simply gets no offers from it. Which scopes the application needs is not checked in this repository.

### Rate limits and politeness

`OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE` (6) and the other `OUTBOUND_*` settings are described in [compliance](compliance.md#politeness-towards-third-party-servers). Do not raise them above what you are willing to defend to the owner of the host.

## Add a source (developers)

A source is an adapter that fetches external data and returns it in a shape the use cases can record. Read ADR [0004](adr/0004-documented-apis-skip-robots.md) first.

### Choose the trait

| Trait | File | Use when | Existing |
|---|---|---|---|
| `Source` | `libs/core/src/domain/source/mod.rs` | The source returns items that map straight to observations: `collect(profiles, now)` fetches, `evaluate(item, profile)` returns `Observation`s for one profile. | CT, DNS CNAME, HTTP probe |
| `CandidateSource` | `libs/core/src/application/triage/mod.rs` | The source returns texts that need keyword routing and maybe the LLM: `rules(profile)` returns the profile's `KeywordSource`, `collect` returns `Candidate`s. | BOAMP, DECP, TED, France Travail, ATS |
| `ContactFinder` | `libs/core/src/domain/contact/mod.rs` | The source finds addresses for a lead domain. | public pages, security.txt, role guess |

Rules for a source:

- Every request goes through `PoliteFetcher` (`libs/fetch`) with a `FetchPurpose`:
  - `Api` only for a documented public API, and only after the source is added to the list in ADR 0004 (this amends the ADR). `Api` skips robots.txt and keeps the rate limit, the public-target check and the User-Agent.
  - `Crawl` for any web page of a company site. It obeys robots.txt.
  - `Probe` for the fixed paths of a profile fingerprint or a well-known file.
- No social network, no job board scraping, no vulnerability scanning (`docs/plan.md`, "Never").
- Never put market criteria in code. Keywords, labels and weights come from the profile.
- Every observation carries a `Provenance` URL. Every signal strength is between 0 and 1.
- Handlers must be safe to run twice.

### Wire it

1. `SourceName` variant and its string in `libs/core/src/domain/values.rs`.
2. If a profile enables it: a field in `Sources` in `libs/core/src/domain/profile/definition.rs`, the checks in `libs/core/src/domain/profile/validation.rs`, the arm in `ProfileDefinition::enables` (`libs/core/src/domain/source/mod.rs`), then regenerate the schema: `UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples`. Add the example fields to `docs/examples/profiles/` if useful.
3. Settings in a new file in `libs/args/src/`, flattened into `Args` (`libs/args/src/lib.rs`), and the lines in `.env.example` and [configuration](configuration.md).
4. The adapter in `libs/sources/src/<name>.rs`, exported from `libs/sources/src/lib.rs`.
5. In `apps/api/src/main.rs` build it with its own `FetchSettings` (body cap, rate floor) and add it to `WiredSources` in `apps/api/src/jobs.rs`.
6. In `apps/api/src/jobs.rs`: add the name to `WIRED_SOURCES`, add a type alias, and add an arm in `JobRouter::run_source` that calls `run_source` (for a `Source`), `triage_source` (for a `CandidateSource`) or `run_contact_finder`. Pick the interval in `Intervals` if it should not use `JOBS_SOURCE_INTERVAL_MINUTES`.
7. If it needs credentials, schedule it only when they are set, as France Travail does (`scheduled_sources`).
8. Metrics: the `source` label is the `SourceName` string, so a new source appears in [metrics](metrics.md) with no extra code beyond the name.

### Test it

- Unit tests next to the code, in `libs/sources/src/<name>/tests.rs`.
- Fixtures: record real answers once and store them under `libs/sources/tests/fixtures/<name>/`. A pipeline test (`libs/sources/tests/<name>_pipeline.rs`) replays them through a fake fetcher and checks the signals and routes against an example profile.
- Live test: `libs/sources/tests/<name>_live.rs`, marked `#[ignore]` and gated by an environment variable such as `LODESTONE_BOAMP_LIVE=1`, optionally `LODESTONE_BOAMP_CONTACT`. It calls the real service through `PoliteFetcher` with the same limits and prints counts. Run it by hand before you open the pull request, never in CI.
- Add a test in `apps/api/src/jobs.rs` that the job is wired and deduplicated, like the existing ones.

## Troubleshooting

| Symptom | Check |
|---|---|
| Binary exits at start with a usage error | A required variable is missing: `DATABASE_NAME`, `AUTH_CLIENT_SECRET`, `OUTBOUND_CONTACT`, `LLM_SECRETS_ENCRYPTION_KEY`. See [configuration](configuration.md). |
| Exit with an encryption key error | `LLM_SECRETS_ENCRYPTION_KEY` must be base64 of exactly 32 bytes. |
| Exit with a lease error | `JOBS_LEASE_SECONDS` must be longer than `JOBS_POLL_INTERVAL_MS`. |
| Exit with a webhook error | `WEBHOOK_URL` needs `WEBHOOK_SECRET` of at least 32 bytes; `WEBHOOK_TIMEOUT_SECONDS` is 1 to 120; `WEBHOOK_PROFILE_IDS` are UUIDs. |
| `relation ... does not exist` in the logs | Migrations were not applied. |
| 401 from the API | Token not issued by `AUTH_ISSUER`, expired, or missing the audience in `AUTH_AUDIENCE`. |
| 403 on a write | The token belongs to a service account. Use a user token. |
| 429 | `RATE_LIMIT_PER_MINUTE` (10 per client IP). Behind a proxy, all clients share one IP unless `SERVER_TRUST_FORWARDED_HEADERS=true` and the proxy overwrites `X-Forwarded-For`. |
| 422 on profile import | Read `details[0].path` and `message`, see [validation errors](profile-authoring.md#validation-errors). |
| 409 on profile import | A profile with this name exists. Pass another `?name=`. |
| `/scalar` or `/swagger` return 404 | `SERVER_ENABLE_API_DOCS` is false (default). |
| Internal port unreachable from kubelet or Prometheus | `SERVER_INTERNAL_HOST` defaults to `127.0.0.1`. The chart sets `0.0.0.0`. |
| `/metrics` is empty | Check that [metrics](metrics.md) is part of your build. `/metrics` on this branch returns an empty body (`apps/api/src/internal_router.rs`). |
| No leads at all | Is the profile active? Does it enable a source? `SELECT * FROM runs ORDER BY started_at DESC LIMIT 20`. Are there candidate hosts (`SELECT count(*) FROM candidate_hosts`)? |
| Probe and DNS find nothing | They read `candidate_hosts`, filled by CT or `import_hosts`. A host is visited once per `OUTBOUND_REVISIT_INTERVAL_HOURS` (720). |
| CT finds nothing | The first run only sets the cursor at the end of each log. Check the log line `ct ingestion pass` for `entries` and `matched`, and the profile labels. Wildcard certificates hide labels. |
| ATS finds nothing | It only scans leads already seen by CT, probe or DNS, and only boards linked from their own pages. |
| France Travail never runs | Log `france travail source disabled`: set both credentials. |
| Log `triage has no usable provider` | No triage tier assigned, provider disabled, or its key cannot be decrypted. Ambiguous candidates are skipped until fixed. |
| Log `qualification skipped, no usable provider` | Same for the qualification tier. |
| Runs with error `llm budget exhausted` | Raise `LLM_RUN_BUDGET_CENTS`, or lower volume. Check the provider prices are set. |
| Leads above the threshold have no draft | Status must be `new`. Check the lead is not `uncertain`, a provider is assigned, and the run was not stopped by the budget. |
| Many `uncertain` leads | The model quotes text not present in the signals. Rewrite `relevance_criteria` to refer to facts the signals state, then reset the leads. |
| Draft queue is empty after approving | Approved and rejected drafts leave the queue. Open the draft by id to see them. |
| Source run `failed` with `rate limited` | Normal under the politeness limit; the next run resumes. Persistent failure: lower the batch or look at the host. |
| DECP run fails or the pod is killed | Check `DECP_MAX_BODY_MB`, the pod memory limit and the timeout (log `decp`). |
| Webhook not delivered | `webhook delivery enabled` in the start log? `SELECT * FROM jobs WHERE kind = 'webhook.deliver'`. Receiver answers 2xx? Suppressed leads are never sent. |
| Job `running` for a long time after a crash | It is reclaimed after `JOBS_LEASE_SECONDS`. |
