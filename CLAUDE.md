# CLAUDE.md - Lodestone

Lodestone is an open-source lead detection service by Ferrislabs. It detects companies likely to buy a product, qualifies them, and queues message drafts for human approval. It never sends a message itself. Plan: `docs/plan.md`. Vocabulary: `docs/glossary.md`. Decisions: `docs/adr/`.

## Repository layout

- `apps/api` — Rust binary, composition root. Public server (`SERVER_PORT`) and internal server (`SERVER_INTERNAL_PORT`, `/health`, `/metrics`).
- `apps/webapp` — React 19 + TanStack (Router, Query, Table, Form).
- `libs/core` — domain, use cases and Postgres adapters (`domain/ → application/ → infrastructure/`).
- `libs/macros` — `#[repository]` registers a Postgres adapter in `infrastructure/registry.rs`; `#[transactional]` opens a transaction and injects repositories into a use case.
- `libs/auth`, `libs/authz`, `libs/handlers`, `libs/rate-limit`, `libs/args`, `libs/common`, `libs/server`, `libs/pagination` — plumbing.
- `migrations/` — SQLx migrations. `.sqlx/` is regenerated with `cargo sqlx prepare --workspace`, never edited by hand.

## Runtime constraint

One binary and one Postgres. No Redis, no broker. Background work goes through the Postgres job queue.

## Commands

```bash
docker compose up -d postgres
cargo build --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
export DATABASE_URL=postgres://ferriskey:ferriskey@localhost:5433/lodestone
cargo nextest run --workspace --run-ignored all   # #[sqlx::test] tests need DATABASE_URL and fail without it
cargo sqlx prepare --workspace                    # after changing a query, needs DATABASE_URL
```

Webapp (`apps/webapp`), package manager pnpm:

```bash
pnpm install
pnpm dev           # vite dev on port 3000
pnpm check         # biome lint + format + organize imports
pnpm build
```

## Backend conventions

- Hexagonal: the domain imports nothing from infrastructure. Repository traits live in `domain/<context>/ports.rs`, adapters in `infrastructure/<context>/postgres/`.
- Errors: `thiserror` enums per boundary, `CoreError` in `libs/common`.
- Repository adapters are tested in `libs/<crate>/tests/` with `#[sqlx::test(migrations = "../../migrations")]`: one fresh database per test.
- Detection profiles: `docs/profiles.md`. Definition types in `libs/core/src/domain/profile/definition.rs`; the JSON Schema is generated (ADR 0002).
- Market-specific criteria (keywords, filters, weights, prompts) never live in code: they belong to detection profiles in Postgres.

## Frontend conventions

- `src/pages/<domain>/feature/`: data fetching (TanStack Query), state, side effects.
- `src/pages/<domain>/ui/`: pure presentational components, no hooks, no fetch.
- Route files in `src/routes/` are thin and call the Feature component. `_app.tsx` is the authenticated layout (FerrisKey OIDC).
- Imports use the `#/*` alias for `src/*`. PascalCase components, kebab-case files. Biome is the source of truth (tabs, double quotes).
