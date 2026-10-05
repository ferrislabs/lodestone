# Detection profiles

A detection profile describes one market: what is sold, which agents scan which sources, how leads are scored, which contacts to prefer and how the LLM qualifies a lead.

## Storage

Profiles live in Postgres. Each edit creates a new revision; earlier revisions are kept. The API validates every definition before storing it. Profiles are never read from a file at runtime. The files under `docs/examples/profiles/` are payloads for the profile API and for tests.

## Schema

The JSON Schema is `docs/schemas/profile.v2.schema.json`. It is generated from `ProfileDefinition` and must not be edited by hand. Regenerate it with:

```
UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples
```

Without `UPDATE_SCHEMA=1` the same test fails if the committed file is stale. The schema describes the shape only. Further rules (regular expressions compile, version extractors have a capture group, no duplicate ids, unique agent ids, at least one agent) are enforced by `ValidatedProfile::parse`.

## Examples

- `docs/examples/profiles/iam-ferriskey.json`: sells FerrisKey and Autharie to organizations running Keycloak or a SaaS IAM. Uses certificate transparency, HTTP probes, CNAME patterns, public tenders and two job agents of the same kind (`ats-fr`, `ats-en`), the second with its own relevance criteria.
- `docs/examples/profiles/ecommerce-replatforming.json`: sells Magento migrations. Uses one HTTP probe with a version extractor, BOAMP and France Travail. It has no IAM-specific field.

## Sections

- `schema_version`: must be 2. See [Version 1 documents](#version-1-documents).
- `identity`: the offer and the target customer, in free text.
- `agents`: the detection agents; at least one is required. An agent searches one source with its own settings (see ADR 0005). Several agents of the same kind are allowed.
  - `id`: slug matching `^[a-z0-9][a-z0-9-]{0,62}$`, unique in the profile.
  - `name`: display name, not empty.
  - `enabled`: defaults to `true`. A disabled agent is kept but not run.
  - `source`: an object whose `kind` selects the source; the other fields are that kind's settings (table below).
  - `llm.relevance_criteria`: optional, not empty when present. Criteria for triaging this agent's candidates; the profile's criteria apply without it. Qualification of a lead always uses the profile's criteria.
- `scoring`: weighted factors, each made of a signal, an optional value and a weight. A lead's score is the sum, over factors, of `weight × measure`. For a normal factor the measure is the highest strength (0 to 1) among the lead's signals of that kind, and of that value when one is set; several matching signals count once. The reserved factor `distinct_signals` measures the number of distinct signal kinds on the lead. The score and its per-factor breakdown are recomputed whenever the lead's signals change, and stored on the lead.
- `contact_ranking`: preferred email local parts, most preferred first.
- `llm`: the qualification threshold and the relevance criteria given to the model.

## Agent kinds

| `kind` | Settings |
| --- | --- |
| `certificate_transparency` | `labels`: hostname labels to look for in certificate logs. |
| `http_probe` | `fingerprints` (path, matchers, optional version extractor, debt markers) and `excluded_products`. |
| `dns_cname` | `patterns`: provider names with the CNAME target suffix that identifies them. |
| `boamp`, `decp`, `ted` | Public procurement sources: `keywords`, `exclusions`, `accept_min_keywords`. |
| `france_travail`, `ats` | Job sources: `keywords`, `exclusions`, `accept_min_keywords`. |

`accept_min_keywords` (default 2, at least 1) is the number of distinct keywords from which a candidate is accepted without the LLM, provided no exclusion matches.

Validation paths include the agent index, for example `agents[1].source.keywords[0]` or `agents[0].llm.relevance_criteria`.

## Dependencies between kinds

- `http_probe` probes hosts found by certificate transparency. Without a `certificate_transparency` agent, it only probes hosts imported by an operator.
- `ats` only visits companies already detected by `certificate_transparency`, `http_probe` or `dns_cname` signals.

## Version 1 documents

Version 1 listed sources as optional fields of a `sources` object. A version 1 document is still accepted: parsing upgrades it to version 2 with one enabled agent per configured source, in this order: certificate transparency, HTTP probe, DNS CNAME, BOAMP, DECP, TED, France Travail, ATS. The agent `id` is the kind in kebab case (`certificate-transparency`, `france-travail`) and the `name` is its display name. Stored revisions are not rewritten; they are upgraded when read, and new revisions are written in version 2.

Until agents run individually, each source runs with the first enabled agent of its kind.
