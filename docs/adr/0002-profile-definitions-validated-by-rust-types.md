# 0002. Profile definitions are validated by Rust types, the JSON Schema is generated

Status: proposed
Date: 2026-10-04

## Context

Detection profiles (#60) are JSON documents stored in Postgres as immutable revisions. Every write must be validated, and a rejected payload must name the faulty field. The issue asks for a versioned JSON Schema. Profiles also carry rules a JSON Schema cannot express without custom keywords: regular expressions that must compile within a size limit, a version extractor with a capture group, `version_below` only when the fingerprint extracts a version, unique fingerprint ids.

## Decision

The Rust types in `libs/core/src/domain/profile/definition.rs` are the source of truth. A write is validated in two passes: typed deserialization with `deny_unknown_fields` through `serde_path_to_error`, which yields the JSON path of a structural error, then semantic checks that return a path and a `ProfileProblem`. Only a `ValidatedProfile` can be stored. The JSON Schema in `docs/schemas/profile.v9.schema.json` is generated from the same types with `schemars` and kept current by a test. `schema_version` in the payload selects the definition version; `1`, `2`, `3`, `4`, `5`, `6`, `7` and `8` are upgraded on read to `9`, the current version.

## Alternatives rejected

- Validate with a JSON Schema engine (`jsonschema` crate) at runtime: a large dependency, a second source of truth to keep in sync with the Rust types, and the semantic rules would still need code.
- Hand-written schema: drifts from the types on the first change.

## Consequences

- Gains: one definition of the format, paths in every error, the schema for API clients and docs at no extra cost.
- Costs: new dependencies `schemars`, `serde_path_to_error` (already in the lockfile through axum), `regex` (already in the lockfile).
- What becomes harder: a breaking change to the definition needs a `schema_version` 2 and a migration of stored revisions, because stored definitions are re-parsed on read.
