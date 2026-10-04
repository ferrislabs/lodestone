# 0001. Keep Oxyde's plumbing, drop its ERP domain

Status: proposed
Date: 2026-10-04

## Context

Lodestone starts from the Oxyde boilerplate (initial commit `2fc8c4f`). Oxyde is a multi-tenant ERP: users, organizations, roles and members, with organization-scoped permission bitfields. Lodestone is a single-tenant lead detection service: one binary, one Postgres, background jobs, a REST API and a web UI behind FerrisKey OIDC.

Baseline on `2fc8c4f`: `cargo build --workspace --all-targets` passes, `cargo nextest run --workspace` gives 197 passed, 22 skipped. The skipped tests are the `#[ignore = "requires docker"]` testcontainers tests in `libs/handlers-organization/tests/organizations_http.rs`, plus the Redis tests in `libs/rate-limit/tests/redis_integration.rs`.

## Decision

Keep the workspace layout, the hexagonal crate pattern, the macros, the auth stack, the HTTP plumbing, the CI and the container build. Remove the ERP domain (organizations, roles, members, IAM provisioning, ERP pages). Replace Redis with an in-process adapter so the runtime needs only Postgres.

| Part | Verdict | Reason |
|------|---------|--------|
| `apps/api` | keep, rename | Thin composition root with public and internal servers (`main.rs`). No SIGTERM handling: `run_server` awaits forever and exits only on error. Graceful shutdown lands with the job queue (#21). |
| `libs/macros` | keep, rename | `#[repository]` and `#[transactional]` (`lib.rs:25`, `lib.rs:132`) are domain-agnostic. |
| `libs/core` | adapt | Keep `domain/ application/ infrastructure/` layout, `RepoFor`/`Backend` registry (`infrastructure/registry.rs:28`), `SharedTx`, `CoreError` mapping. Remove `organization`, `role`, `member`. Keep `user` keyed on OIDC `sub` to attribute status changes and draft approvals. |
| `libs/common` | keep | `Config`, `Secret` wrapper, `CoreError`. |
| `libs/args` | adapt | Clap + env pattern kept. Defaults renamed. Settings for Lodestone added in #23. Redis args removed. |
| `libs/server` | adapt | `run_server` gains graceful shutdown (#21). |
| `libs/auth` | keep | FerrisKey JWKS validation, RS256, 300 s key cache (`infrastructure/ferriskey.rs:30`). Generic OIDC. |
| `libs/authz` | adapt | `LocalPolicyEngine` and AuthZen types are generic. The `oxid.permissions` bitfield is fed from organization roles (`libs/core/src/application/policy.rs`); without organizations, authorization uses FerrisKey realm roles only. The super-admin default `oxid:admin` is renamed. |
| `libs/iam` | remove | Provisions organizations and memberships in FerrisKey. No use in a single-tenant tool. |
| `libs/handlers` | adapt | Keep bearer auth middleware, `ApiError` mapping, `DataEnvelope`, rate-limit middleware. Drop organization-scoped rate-limit keys. |
| `libs/handlers-organization` | remove | ERP endpoints. Its testcontainers harness (`tests/harness/mod.rs`) is the model for future HTTP integration tests. |
| `libs/pagination` | keep | Generic, overflow-safe. |
| `libs/rate-limit` | adapt | Port `RateLimiter` kept. Redis adapter replaced by an in-process sliding window adapter. Redis breaks the one binary + Postgres constraint. |
| `migrations/` | replace | The 9 migrations build the ERP schema. Lodestone was never deployed, so they are squashed into one `users` migration. `.sqlx/` regenerated. |
| `.github/workflows/docker.yaml` | keep | `rust-check` runs fmt, clippy, test with toolchain 1.95.0 and SHA-pinned actions, then multi-arch image build and push. Gap: docker-dependent tests are `#[ignore]` and never run in CI (#19). |
| `.github/workflows/webapp.yaml` | keep | biome, tsc, vitest, build. |
| `.github/workflows/codspeed.yaml` | keep | Benchmarks on PRs. |
| `Dockerfile` | keep, rename | cargo-chef build, non-root runtime, migrator stage. User `oxid` renamed. |
| `docker-compose.yml` | adapt | Keep Postgres and the optional SigNoz stack. Remove Redis. |
| `deploy/` | keep | OTel collector and SigNoz configs for local use. No Helm chart yet (#57). |
| `apps/webapp` | adapt | Keep router, OIDC (`oidc-client-ts`, `react-oidc-context`), runtime config, auth gate, Feature/UI split. Remove `pages/customers` and the ERP dashboard in `pages/home`. |
| `README.md`, `CLAUDE.md` | rewrite | Both describe Oxid. |

## Alternatives rejected

- Start from an empty workspace: loses the tested auth stack, the macros, the CI and the image build.
- Keep organizations for future multi-tenancy: no requirement asks for tenants. Profiles (#60) already separate markets.
- Keep Redis for rate limiting: adds a second runtime dependency. Inbound API traffic is a few operators. Outbound politeness limits (#24) need per-host state, which an in-process adapter covers for one replica.

## Consequences

- Gains: one runtime dependency (Postgres). Less code to maintain. The test harness, auth and CI are reused as they are.
- Costs: the in-process rate limiter is per replica. Running several replicas multiplies the inbound limit and splits outbound per-host limits.
- What becomes harder: horizontal scaling. A shared limiter needs a Postgres adapter behind the same port.

## Open questions

- CI pins Rust 1.95.0. There is no `rust-toolchain.toml`, so local builds use whatever is installed (1.99.0 here). To settle in #19.
- `/metrics` on the internal port returns an empty body (`apps/api/src/internal_router.rs`). Metrics are exported over OTLP only. Prometheus exposition is #58.
- The FerrisKey realm and client for Lodestone are not created. Defaults point to `http://localhost:3333/realms/oxid`.
- Whether `CODSPEED_TOKEN` is configured for this repository is not verified.
