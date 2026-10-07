# Lodestone

Open-source lead detection. Lodestone finds companies likely to buy a product, qualifies them, and queues message drafts for a human to approve. It never sends a message itself.

Market criteria live in detection profiles stored in Postgres. The first profile targets IAM for FerrisKey and Autharie. Plan: `docs/plan.md`. Vocabulary: `docs/glossary.md`.

Backend in Rust (Axum + SQLx + PostgreSQL), frontend in React 19 + TanStack.

## Workspace layout

```
apps/
  api/                 # Rust HTTP API binary (Axum), composition root
  webapp/              # React 19 frontend (TanStack Router/Query/Table/Form)
libs/
  args/                # CLI / env args (clap)
  auth/                # OIDC token validation (FerrisKey)
  authz/               # In-process policy engine (AuthZen types)
  common/              # Shared config, Secret, CoreError
  core/                # Domain, use cases, Postgres adapters
  handlers/            # Axum middleware, error mapping, response envelope
  macros/              # #[repository] and #[transactional]
  pagination/          # Pagination types
  rate-limit/          # Rate limiter port, in-process adapter
  server/              # Axum server helpers
docs/                  # Plan, glossary, ADRs
deploy/                # Local observability configs (SigNoz, OTel collector)
migrations/            # SQLx migrations
```

## Prerequisites

- Rust (edition 2024) and `cargo`
- `pnpm` (frontend)
- Docker + Docker Compose (Postgres + observability stack)

## Local stack

A `docker-compose.yml` at the root spins up Postgres and the full SigNoz observability stack (ClickHouse, query-service, frontend, OpenTelemetry collector).

```bash
docker compose up -d
```

Exposed ports on the host:

| Service              | Port        | Notes                                          |
| -------------------- | ----------- | ---------------------------------------------- |
| PostgreSQL           | 5433        | mapped to container `5432` (avoid clash)       |
| OTLP gRPC            | 4317        | matches `OTLP_ENDPOINT` in `.env`              |
| OTLP HTTP            | 4318        |                                                |
| SigNoz UI            | 8080        | http://localhost:8080                          |

The first boot takes 1–2 minutes while `signoz-telemetrystore-migrator` provisions ClickHouse.

## Running the API

```bash
cargo run -p api
```

The API binary exposes two HTTP servers:

- **Public API** on `SERVER_PORT` (default `3456`) — business endpoints, `/scalar`, `/swagger`.
- **Internal API** on `SERVER_INTERNAL_PORT` (default `3457`) — `/health`, `/metrics`. Not meant to be exposed publicly.

Common flags / env vars (see `libs/args/src/`):

| Env                    | Flag                       | Default                            |
| ---------------------- | -------------------------- | ---------------------------------- |
| `SERVER_HOST`          | `--server-host`            | `0.0.0.0`                          |
| `SERVER_PORT`          | `--server-port`            | `3456`                             |
| `SERVER_INTERNAL_PORT` | `--server-internal-port`   | `3457`                             |
| `SERVER_PUBLIC_URL`    | `--server-public-url`      | `http://localhost:3456`            |
| `SERVER_WEBAPP_URL`    | `--server-webapp-url`      | `http://localhost:3000`            |
| `DATABASE_NAME`        | `--database-name`          | required                           |
| `ACTIVE_OBSERVABILITY` | `--active-observability`   | `false`                            |
| `OTLP_ENDPOINT`        | `--otlp-endpoint`          | `http://localhost:4317`            |
| `METRICS_ENDPOINT`     | `--metrics-endpoint`       | `http://localhost:4317`            |
| `RATE_LIMIT_PER_MINUTE` | `--rate-limit-per-minute` | `10` |
| `JOBS_WORKERS` | `--jobs-workers` | `4` |
| `JOBS_LEASE_SECONDS` | `--jobs-lease-seconds` | `300` |
| `JOBS_POLL_INTERVAL_MS` | `--jobs-poll-interval-ms` | `1000` |
| `JOBS_MAX_ATTEMPTS` | `--jobs-max-attempts` | `5` |
| `JOBS_RETRY_BASE_SECONDS` | `--jobs-retry-base-seconds` | `10` |
| `JOBS_SHUTDOWN_GRACE_SECONDS` | `--jobs-shutdown-grace-seconds` | `20` |
| `JOBS_SOURCE_INTERVAL_MINUTES` | `--jobs-source-interval-minutes` | `60` |
| `LISTENER_IDLE_SECONDS` | `--listener-idle-seconds` | `15` |
| `LISTENER_LOCK_RETRY_SECONDS` | `--listener-lock-retry-seconds` | `15` |
| `PROBE_CONCURRENCY` | `--probe-concurrency` | `8` |
| `DNS_CONCURRENCY` | `--dns-concurrency` | `16` |
| `OUTBOUND_CONTACT` | `--outbound-contact` | required, email or https URL |
| `OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE` | `--outbound-requests-per-host-per-minute` | `6` |
| `OUTBOUND_REVISIT_INTERVAL_HOURS` | `--outbound-revisit-interval-hours` | `720` |
| `OUTBOUND_REQUEST_TIMEOUT_SECONDS` | `--outbound-request-timeout-seconds` | `15` |
| `CT_LOG_LIST_URL` | `--ct-log-list-url` | Chrome v3 log list |
| `CT_MAX_LOGS` | `--ct-max-logs` | `8` |
| `CT_TILES_PER_RUN` | `--ct-tiles-per-run` | `64` |
| `CT_MAX_LAG_TILES` | `--ct-max-lag-tiles` | `256` |
| `CT_REQUESTS_PER_HOST_PER_MINUTE` | `--ct-requests-per-host-per-minute` | `120` |
| `CONTACT_PAGE_PATHS` | `--contact-page-paths` | `/,/contact,/mentions-legales,/legal,/about` |
| `CONTACT_BATCH_SIZE` | `--contact-batch-size` | `10` |
| `CONTACT_MAX_RATE_LIMIT_WAIT_SECONDS` | `--contact-max-rate-limit-wait-seconds` | `75` |
| `LLM_SECRETS_ENCRYPTION_KEY` | `--llm-secrets-encryption-key` | required, base64 of 32 bytes |
| `LLM_RUN_BUDGET_CENTS` | `--llm-run-budget-cents` | `500` |

## Source providers

Every data source is a provider of the catalog (Sources > Catalogue): BOAMP, DECP, TED, France Travail, career pages, certificate transparency, DNS CNAME, HTTP probe, public contacts and company research. A source only runs while its provider is `ready`, that is configured and successfully tested.

- **Public providers** (no authentication) are created and tested automatically at startup, so a fresh install works without manual steps. A provider whose test fails is `failing` and its source is skipped (`provider not ready` in the logs, « Provider non prêt » in the Activity view) until a later test passes. Failed public providers are retested at each start, or on demand from the catalog.
- **France Travail** needs an OAuth2 client id and secret entered in the catalog. `FRANCE_TRAVAIL_CLIENT_ID`, `FRANCE_TRAVAIL_CLIENT_SECRET`, `FRANCE_TRAVAIL_TOKEN_URL` and `FRANCE_TRAVAIL_API_BASE_URL` are an optional seed only: on the first start, when no France Travail configuration exists yet, they create it (the secret is stored encrypted) and the connection is tested. Once the configuration exists the variables are ignored, so edit credentials in the catalog.
- The URL variables of BOAMP, DECP, TED, certificate transparency and the company registry seed the catalog URL used by the connection test the same way; collections still read the variables.
- "Lancer maintenant" on a source whose provider is not ready answers `409` with a French message.

## Database migrations

```bash
cargo install sqlx-cli --no-default-features --features rustls,postgres
sqlx migrate run
```

## Frontend

```bash
cd apps/webapp
pnpm install
pnpm dev          # vite dev on port 3000
pnpm check        # biome lint + format + organize imports
pnpm build
```

See `CLAUDE.md` for the architecture conventions (Feature/UI split, routing, tech stack).

## Observability

When `ACTIVE_OBSERVABILITY=true`, the API exports traces, metrics, and logs over OTLP gRPC. With the compose stack running, open SigNoz at http://localhost:8080 to inspect them.
