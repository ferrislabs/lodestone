# Runtime configuration

Every setting is an environment variable read by the API binary (`apps/api`), or the matching command line flag. Definitions are in `libs/args/src/*.rs`; the template is `.env.example`. A local `.env` file is loaded at start (`dotenvy`, `apps/api/src/main.rs`).

In the tables, a bare file name in the Source column is relative to `libs/args/src/`. "Required" means the process refuses to start without a value. A variable marked secret must not be written in a ConfigMap or committed. In the Helm chart, secrets come from Kubernetes Secrets referenced by name (`secrets.<ENV_NAME>: {name, key}` in `values.yaml`), everything else goes in `config.<ENV_NAME>`. See [Helm guide](deploy/helm.md).

## Server

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `SERVER_HOST` | `0.0.0.0` | no | no | Interface of the public server. | `server.rs` |
| `SERVER_PORT` | `3456` | no | no | Port of the public server (business API). | `server.rs` |
| `SERVER_INTERNAL_HOST` | `127.0.0.1` | no | no | Interface of the internal server (`/health`, `/metrics`). It has no authentication, hence loopback by default. | `server.rs` |
| `SERVER_INTERNAL_PORT` | `3457` | no | no | Port of the internal server. | `server.rs` |
| `ALLOWED_ORIGINS` | empty | no | no | Comma separated CORS origins. An invalid origin makes the router fail to build. | `server.rs`, `apps/api/src/router.rs` |
| `SERVER_ENABLE_API_DOCS` | `false` | no | no | Serve `/scalar`, `/swagger` and the OpenAPI document. They sit outside authentication. | `server.rs` |
| `SERVER_TRUST_FORWARDED_HEADERS` | `false` | no | no | Trust `X-Forwarded-For`, `X-Real-IP`, `Forwarded` to identify a client for rate limiting. Only safe behind a proxy that overwrites them. | `server.rs` |
| `RATE_LIMIT_PER_MINUTE` | `10` | no | no | Requests per minute per client IP on the public server. | `rate_limit.rs` |
| `LOG_FILTER` | `info` | no | no | `tracing` filter directive. | `log.rs` |
| `LOG_JSON` | off | no | no | Log in JSON format. | `log.rs` |
| `ACTIVE_OBSERVABILITY` | `false` | no | no | Export traces, metrics and logs over OTLP gRPC. | `observability.rs` |
| `OTLP_ENDPOINT` | none | with `ACTIVE_OBSERVABILITY=true` | no | Trace and log collector. | `observability.rs` |
| `METRICS_ENDPOINT` | none | with `ACTIVE_OBSERVABILITY=true` | no | Metrics collector. | `observability.rs` |

Prometheus metrics on `/metrics` are described in [metrics](metrics.md).

## Auth

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `AUTH_ISSUER` | `http://localhost:3333/realms/lodestone` | no | no | OIDC issuer (FerrisKey realm URL). | `auth.rs` |
| `AUTH_CLIENT_ID` | `lodestone` | no | no | Client id used to talk to the identity provider. | `auth.rs` |
| `AUTH_AUDIENCE` | none | no | no | Expected audience. When set, tokens without it are rejected. | `auth.rs` |
| `AUTH_CLIENT_SECRET` | none | yes | yes | Client secret used to talk to the identity provider. | `auth.rs` |

## Database

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `DATABASE_HOST` | `localhost` | no | no | Postgres host. | `database.rs` |
| `DATABASE_PORT` | `5432` | no | no | Postgres port. | `database.rs` |
| `DATABASE_NAME` | none | yes | no | Database name. | `database.rs` |
| `DATABASE_USER` | `lodestone` | no | no | Database user. | `database.rs` |
| `DATABASE_PASSWORD` | `lodestone` | no (see inconsistencies) | yes | Database password. | `database.rs` |

The API does not run migrations. Apply them with `sqlx migrate run` or the chart's migration Job.

## Jobs

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `JOBS_WORKERS` | `4` | no | no | Concurrent workers per process. Must be at least 1. | `jobs.rs` |
| `JOBS_LEASE_SECONDS` | `300` | no | no | Time a worker holds a job before another may reclaim it. Must be longer than `JOBS_POLL_INTERVAL_MS`, otherwise the process refuses to start. | `jobs.rs`, `libs/common/src/settings.rs` |
| `JOBS_POLL_INTERVAL_MS` | `1000` | no | no | Idle worker wait between queue polls. | `jobs.rs` |
| `JOBS_MAX_ATTEMPTS` | `5` | no | no | Attempts of a webhook delivery job. Source and qualification jobs use a fixed 3 (see below). | `jobs.rs`, `apps/api/src/jobs.rs`, `libs/core/src/application/sources/mod.rs` |
| `JOBS_RETRY_BASE_SECONDS` | `10` | no | no | Base of the exponential backoff (doubles per attempt, capped at 3600 s). | `jobs.rs`, `libs/core/src/domain/job/mod.rs` |
| `JOBS_SHUTDOWN_GRACE_SECONDS` | `20` | no | no | Time running jobs get to finish on shutdown. | `jobs.rs` |
| `JOBS_SOURCE_INTERVAL_MINUTES` | `60` | no | no | Minutes between two runs of a source (all sources except CT and DECP) and between two qualification runs. | `jobs.rs`, `apps/api/src/jobs.rs` |

## Outbound requests

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `OUTBOUND_CONTACT` | none | yes | no | Email address or `https://` URL. Put in the `User-Agent` of every outbound request: `Lodestone/<version> (+<contact>)`. | `outbound.rs`, `libs/fetch/src/fetcher.rs` |
| `OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE` | `6` | no | no | Maximum requests to one host per minute, per fetcher instance. Raised to at least 30 for BOAMP and TED and 120 for DECP, fixed at 180 for France Travail. | `outbound.rs`, `apps/api/src/main.rs` |
| `OUTBOUND_REVISIT_INTERVAL_HOURS` | `720` | no | no | Hours before the same host is probed, resolved or searched for contacts again. | `outbound.rs` |
| `OUTBOUND_REQUEST_TIMEOUT_SECONDS` | `15` | no | no | Timeout of one outbound request. DECP raises it to at least 900 s. | `outbound.rs`, `apps/api/src/main.rs` |

## LLM

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `LLM_SECRETS_ENCRYPTION_KEY` | none | yes | yes | Base64 of 32 random bytes (`openssl rand -base64 32`). Encrypts provider API keys at rest with AES-256-GCM. | `llm.rs`, `libs/core/src/infrastructure/crypto/` |
| `LLM_RUN_BUDGET_CENTS` | `500` | no | no | Maximum LLM spend of one run per profile, in US cents. Must be at least 1. | `llm.rs` |

Providers, models, prices and tier assignment are stored in Postgres and managed through the API and the UI, not through environment variables.

## Sources

### Certificate Transparency

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `CT_LOG_LIST_URL` | `https://www.gstatic.com/ct/log_list/v3/log_list.json` | no | no | Chrome log list giving the tiled logs. | `ct.rs` |
| `CT_MAX_LOGS` | `4` | no | no | Number of tiled logs read, in log list order by description. | `ct.rs` |
| `CT_TILES_PER_RUN` | `16` | no | no | Data tiles (256 entries each) read per log per run. | `ct.rs` |
| `CT_MAX_LAG_TILES` | `256` | no | no | A log further behind than this skips ahead. | `ct.rs` |
| `CT_INTERVAL_SECONDS` | `60` | no | no | Seconds between two CT runs. | `ct.rs` |

### Contacts

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `CONTACT_PAGE_PATHS` | `/,/contact,/mentions-legales,/legal,/about` | no | no | Comma separated paths crawled on each lead domain. | `contacts.rs` |
| `CONTACT_BATCH_SIZE` | `10` | no | no | Lead domains handled per contact run. | `contacts.rs` |
| `CONTACT_MAX_RATE_LIMIT_WAIT_SECONDS` | `75` | no | no | Longest per-host rate limit wait accepted while crawling one domain; a longer wait ends the run. | `contacts.rs` |

### BOAMP

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `BOAMP_BASE_URL` | `https://boamp-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/boamp/records` | no | no | OpenDataSoft records endpoint. | `boamp.rs` |
| `BOAMP_LOOKBACK_DAYS` | `7` | no | no | Only notices published in the last N days. | `boamp.rs` |
| `BOAMP_MAX_PAGES` | `20` | no | no | Maximum pages of 50 notices per run. | `boamp.rs` |

### DECP

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `DECP_DATASET_API_URL` | `https://www.data.gouv.fr/api/1/datasets/donnees-essentielles-de-la-commande-publique-fichiers-consolides/` | no | no | data.gouv.fr API URL of the consolidated dataset. | `decp.rs` |
| `DECP_ANNUAIRE_URL` | `https://api-lannuaire.service-public.fr/api/explore/v2.1/catalog/datasets/api-lannuaire-administration/records` | no | no | Buyer SIRET to website. | `decp.rs` |
| `DECP_ENTREPRISES_URL` | `https://recherche-entreprises.api.gouv.fr/search` | no | no | SIRET to company name. | `decp.rs` |
| `DECP_LOOKBACK_DAYS` | `31` | no | no | Monthly files covering the last N days are downloaded. | `decp.rs` |
| `DECP_NOTIFIED_WITHIN_DAYS` | `180` | no | no | Only contracts notified in the last N days are kept. | `decp.rs` |
| `DECP_INTERVAL_HOURS` | `24` | no | no | Hours between two DECP runs. Each run downloads the monthly files again. | `decp.rs` |
| `DECP_MAX_BODY_MB` | `256` | no | no | Largest monthly file accepted, in MB. A full month is about 140 MB. | `decp.rs` |

### TED

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `TED_BASE_URL` | `https://api.ted.europa.eu/v3/notices/search` | no | no | TED Search API endpoint (POST, no key). | `ted.rs` |
| `TED_LOOKBACK_DAYS` | `7` | no | no | Only notices published in the last N days. | `ted.rs` |
| `TED_MAX_PAGES` | `10` | no | no | Maximum pages of 100 notices per run. | `ted.rs` |
| `TED_BUYER_COUNTRIES` | `FRA` | no | no | Comma separated ISO 3166-1 alpha-3 codes. Empty for every country. | `ted.rs` |

### France Travail

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `FRANCE_TRAVAIL_CLIENT_ID` | none | no | yes | OAuth2 client id of the partner application. The source is disabled and its job not scheduled unless both id and secret are non-empty. | `france_travail.rs` |
| `FRANCE_TRAVAIL_CLIENT_SECRET` | none | no | yes | OAuth2 client secret. | `france_travail.rs` |
| `FRANCE_TRAVAIL_TOKEN_URL` | `https://entreprise.francetravail.fr/connexion/oauth2/access_token?realm=%2Fpartenaire` | no | no | Token endpoint (client credentials grant). | `france_travail.rs` |
| `FRANCE_TRAVAIL_API_BASE_URL` | `https://api.francetravail.io/partenaire/offresdemploi/v2` | no | no | API base, `/offres/search` is appended. | `france_travail.rs` |
| `FRANCE_TRAVAIL_PUBLISHED_SINCE_DAYS` | `7` | no | no | Mapped to the closest value the API accepts (1, 3, 7, 14, 31). | `france_travail.rs` |
| `FRANCE_TRAVAIL_MAX_PAGES` | `20` | no | no | Maximum pages of 150 offers per keyword and run. | `france_travail.rs` |

### ATS job boards

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `ATS_BATCH_SIZE` | `20` | no | no | Lead domains scanned per run. | `ats.rs` |
| `ATS_REVISIT_DAYS` | `7` | no | no | Days before a lead domain is scanned again. | `ats.rs` |
| `ATS_MAX_CAREER_PATHS` | `6` | no | no | Careers pages tried after the homepage to find a board link. | `ats.rs` |

HTTP probe and DNS CNAME have no variable of their own: they read the candidate hosts found by CT or imported, and use `OUTBOUND_*`.

## Webhooks

| Variable | Default | Required | Secret | Description | Source |
|---|---|---|---|---|---|
| `WEBHOOK_URL` | none | no | yes | Endpoint receiving a signed POST per validated lead. Disabled when unset or empty. Must be an absolute http or https URL. | `webhook.rs` |
| `WEBHOOK_SECRET` | none | with `WEBHOOK_URL` | yes | HMAC-SHA256 key, at least 32 bytes (`openssl rand -hex 32`). | `webhook.rs` |
| `WEBHOOK_PROFILE_IDS` | empty | no | no | Comma separated profile UUIDs to deliver. Every profile when empty. | `webhook.rs` |
| `WEBHOOK_TIMEOUT_SECONDS` | `10` | no | no | Delivery timeout, 1 to 120. | `webhook.rs` |

Delivery format and verification: [webhooks](webhooks.md).

## Webapp

Read by `apps/webapp` (`apps/webapp/.env.example`, `apps/webapp/src/lib/runtime-config.ts`). None is secret.

| Variable | Default in `.env.example` | When read | Description |
|---|---|---|---|
| `VITE_API_URL` | `http://localhost:3456` | runtime, overridable by `/config.json` | API base URL. |
| `VITE_OIDC_AUTHORITY` | `https://iam.example.com/realms/lodestone` | runtime, overridable by `/config.json` | OIDC issuer the browser logs in to. |
| `VITE_OIDC_CLIENT_ID` | `lodestone-webapp` | build | Public OIDC client of the webapp. |
| `VITE_OIDC_REDIRECT_URI` | `http://localhost:3000/` | build | Redirect after login. |
| `VITE_OIDC_POST_LOGOUT_REDIRECT_URI` | `http://localhost:3000/` | build | Redirect after logout. |
| `VITE_OIDC_SCOPE` | `openid profile email` | build | Requested scopes. |

## Values fixed in code

These are not configurable. Source: `apps/api/src/main.rs`, `apps/api/src/jobs.rs`, `libs/fetch/src/settings.rs`.

| Value | Setting |
|---|---|
| Response body cap | 2 MiB; 16 MiB for BOAMP, TED, France Travail and ATS; `DECP_MAX_BODY_MB` for DECP |
| Redirects | up to 5 hops, same registrable domain only, never on POST |
| robots.txt cache | 24 hours; 10 minutes after a failed fetch |
| CT log list cache | 6 hours |
| DNS CNAME run | 500 hosts, 16 concurrent |
| HTTP probe run | 200 hosts, 8 concurrent, HTTPS only |
| France Travail rate | 180 requests per minute per host |
| ATS and DECP rate limit wait | 75 s |
| Source job and qualification job | 3 attempts, then the next run is scheduled |
| Qualification run | 20 leads per profile, 4096 max output tokens |
| Triage | 1024 max output tokens |

## Not covered / inconsistencies

- `DATABASE_PASSWORD`: `.env.example` and the flag help say it is required, but `libs/args/src/database.rs` sets the default `lodestone`. A deployment that forgets it starts with that default.
- `OTLP_ENDPOINT`, `METRICS_ENDPOINT`: the README table gives the default `http://localhost:4317`. The flags have no default (`libs/args/src/observability.rs`); the value is used only in `Default`, which clap does not call. With `ACTIVE_OBSERVABILITY=true` and no value, `apps/api/src/logger.rs` returns an internal error.
- `ALLOWED_ORIGINS`, `SERVER_INTERNAL_HOST`, `LOG_JSON` are read but absent from `.env.example`. The README variable table also omits them and every source, webhook and ATS variable.
- `ServerArgs::default()` returns ports 3333 and 3334 (`libs/args/src/server.rs`); the flags default to 3456 and 3457. Only the flag defaults take effect.
- `JOBS_MAX_ATTEMPTS` is documented as "attempts before a failing job is marked as failed for good". In code it only sets the attempts of webhook jobs (`libs/handlers/src/state.rs`). Source and qualification jobs have fixed attempts and reschedule instead of failing after the last one (`apps/api/src/jobs.rs`).
- `FetchSettings.source_quotas` exists in `libs/fetch`, but `apps/api/src/main.rs` never fills it. Only per-host limits apply.
