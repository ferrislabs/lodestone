# Detection profiles

A detection profile describes one market: what is sold, which sources to scan, how leads are scored, which contacts to prefer and how the LLM qualifies a lead.

## Storage

Profiles live in Postgres. Each edit creates a new revision; earlier revisions are kept. The API validates every definition before storing it. Profiles are never read from a file at runtime. The files under `docs/examples/profiles/` are payloads for the profile API and for tests.

## Schema

The JSON Schema is `docs/schemas/profile.v1.schema.json`. It is generated from `ProfileDefinition` and must not be edited by hand. Regenerate it with:

```
UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples
```

Without `UPDATE_SCHEMA=1` the same test fails if the committed file is stale. The schema describes the shape only. Further rules (regular expressions compile, version extractors have a capture group, no duplicate ids, at least one source) are enforced by `ValidatedProfile::parse`.

## Examples

- `docs/examples/profiles/iam-ferriskey.json`: sells FerrisKey and Autharie to organizations running Keycloak or a SaaS IAM. Uses certificate transparency, HTTP probes, CNAME patterns, public tenders and job boards.
- `docs/examples/profiles/ecommerce-replatforming.json`: sells Magento migrations. Uses one HTTP probe with a version extractor, BOAMP and France Travail. It has no IAM-specific field.

## Sections

- `schema_version`: must be 1.
- `identity`: the offer and the target customer, in free text.
- `sources`: the enabled sources; at least one is required.
  - `certificate_transparency`: hostname labels to look for in certificate logs.
  - `http_probe`: fingerprints (path, matchers, optional version extractor, debt markers) and excluded products.
  - `dns_cname`: provider names with the CNAME target suffix that identifies them.
  - `boamp`, `decp`, `ted`: public procurement sources, with keywords and exclusions.
  - `france_travail`, `ats`: job sources, with keywords and exclusions.
  - Keyword sources also take `accept_min_keywords` (default 2, at least 1): the number of distinct keywords from which a candidate is accepted without the LLM, provided no exclusion matches.
- `scoring`: weighted factors, each made of a signal, an optional value and a weight. A lead's score is the sum, over factors, of `weight × measure`. For a normal factor the measure is the highest strength (0 to 1) among the lead's signals of that kind, and of that value when one is set; several matching signals count once. The reserved factor `distinct_signals` measures the number of distinct signal kinds on the lead. The score and its per-factor breakdown are recomputed whenever the lead's signals change, and stored on the lead.
- `contact_ranking`: preferred email local parts, most preferred first.
- `llm`: the qualification threshold and the relevance criteria given to the model.
