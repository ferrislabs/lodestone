# 0005. Profiles are made of detection agents

Status: accepted
Date: 2026-10-05

## Context

A profile definition (schema version 1) lists every source as an optional field of `sources`: certificate transparency, DNS CNAME, HTTP probe, BOAMP, DECP, TED, France Travail and ATS. Each source has one configuration per profile, and the LLM relevance criteria are shared by all sources.

Writing a profile through the UI meant facing every field of every source at once. A profile could not search the same source twice with different settings, for example job offers in French and in English, and a criterion written for public tenders also applied to job offers.

## Decision

A profile owns a list of **agents**. An agent searches one source with its own settings.

- An agent has an `id` (slug, unique in the profile), a `name`, an `enabled` flag, a `source` and optional `llm` settings.
- `source` is a tagged union on `kind`, one variant per source: `certificate_transparency`, `dns_cname`, `http_probe`, `boamp`, `decp`, `ted`, `france_travail`, `ats`. Each variant carries the settings that source had in schema version 1.
- A profile can hold several agents of the same kind. At least one agent is required, and agent ids are unique.
- An agent's `llm.relevance_criteria` is used when its candidates go through LLM triage. Without it, the profile's criteria apply. Qualification of a lead (#43) keeps using the profile's criteria, because a lead gathers signals from several agents.
- Identity, scoring, contact ranking and `llm.qualification_threshold` stay on the profile, so a lead is scored the same way whichever agent found it.
- Every signal and every run records the agent that produced it. Metrics carry an `agent` label (bounded by the number of agents).

Schema version 2 replaces `sources` with `agents`. A version 1 document is still accepted: parsing upgrades it to version 2 with one agent per configured source (`id` = the source kind, `name` = its display name). Stored revisions are not rewritten, since revisions are immutable; they are upgraded when read. New revisions are written in version 2.

## Dependencies between kinds

- `http_probe` probes hosts found by certificate transparency. A profile with an HTTP probe agent and no certificate transparency agent only probes hosts imported by an operator.
- `ats` only visits companies already detected by `certificate_transparency`, `http_probe` or `dns_cname` signals (#49).

The UI states these dependencies on the agent forms.

## Consequences

- The profile editor shows four tabs: Identity, Agents, Scoring, Settings. Adding an agent starts by choosing its kind; only that kind's fields are shown.
- Sources iterate over enabled agents of their kind instead of reading one optional field per profile. Keyword sources route each candidate with the rules of each agent of that kind.
- Signals, runs and metrics gain an agent dimension. Existing rows have no agent (`NULL`).
- Example profiles and documentation move to version 2. Importing a version 1 file keeps working.
