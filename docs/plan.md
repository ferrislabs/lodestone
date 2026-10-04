# Implementation plan

Lodestone detects companies likely to buy a product, qualifies them, and queues message drafts for a human to approve. It never sends a message itself.

Market-specific criteria (keywords, filters, HTTP fingerprints, CNAME patterns, scoring weights, prompt context) live in detection profiles stored in Postgres (#60). IAM for FerrisKey and Autharie is the first profile, not a built-in. Several profiles run at once.

Runtime: one Rust binary and one Postgres. Background work runs on a job queue in Postgres. REST API documented with OpenAPI, web UI on top of it, authentication through FerrisKey OIDC.

Each phase is a GitHub milestone. Within Phase 1, pull requests are stacked in this order: #16, #17, #18, #19, #22, #23, #60, #20, #21.

## Definition of done, every phase

- Unit and integration tests green, including docker-dependent ones.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Documentation updated.
- Behavior checked on real data when the phase touches an external source.
- A short report on the milestone: what was done, what stays uncertain, which assumptions were made.

## Phase 1: audit, plan, skeleton, data model

| Issue | Content | Depends on |
|-------|---------|------------|
| #16 | Oxyde audit, ADR 0001 | |
| #17 | This plan and the glossary | #16 |
| #18 | Remove the ERP domain, rename to Lodestone | #16 |
| #19 | Rust quality gate runs docker-dependent tests | #18 |
| #22 | Normalized domain value type | #18 |
| #23 | Runtime settings | #18 |
| #60 | Detection profiles and revisions in Postgres | #18 |
| #20 | Leads, signals, contacts, assessments, runs, suppression list | #18, #22, #60 |
| #21 | Job queue with leases and resume after restart | #18 |

Exit: a binary that boots on Postgres only, migrations for the full model, profiles stored and revised, jobs survive a restart.

## Phase 2: product detection (HTTP, DNS, CT)

#24 polite outbound HTTP client, #25 source port and run recording, #26 fixture corpus, #27 HTTP fingerprint probe, #28 DNS CNAME check, #29 CT source evaluation (ADR), #30 CT ingestion.

Unverified: Certstream availability, crt.sh rate limits, CT log endpoints. #29 checks them before #30 starts.

## Phase 3: contact email extraction

#31 public pages with deobfuscation, #32 security.txt as a hint only, #33 role address fallback after MX check and ranking.

## Phase 4: deduplication and scoring

#34 merge signals per profile and domain, #35 weighted scoring from profile factors, #36 suppression list enforcement.

## Phase 5: LLM layer

#37 `LlmProvider` port, #38 Mistral and OpenAI-compatible adapters, #61 providers and encrypted API keys in Postgres, #39 schema validation, retries, timeouts, budget, #40 versioned base prompts and isolation of fetched content, #41 cache, #42 cheap triage, #43 deep qualification, #44 evidence check.

## Phase 6: public tenders

#45 BOAMP, #46 DECP, #47 TED (later).

Unverified: DILA open data API shape and limits, DECP dataset URL and format, TED API.

## Phase 7: job offers

#48 France Travail, #49 ATS career pages (Greenhouse, Lever, Ashby) for detected companies.

Unverified: France Travail scopes, current ATS public endpoints.

## Phase 8: API, UI, auth, exports

#53 FerrisKey OIDC on the API, #50 leads API, #51 profiles API, #62 LLM providers API, #52 draft validation queue, #54 leads UI, #55 drafts, profiles and providers UI, #56 exports and webhooks.

## Phase 9: Helm, observability, documentation

#57 Helm chart, #58 Prometheus metrics, #59 operator, profile authoring and compliance docs.

## Never

- Send an email automatically.
- Add a social network source or scrape LinkedIn, Welcome to the Jungle or Indeed.
- Hard-code keywords, weights or thresholds.
- Use an endpoint or API that has not been checked.
- Scan for vulnerabilities or access anything non-public.
