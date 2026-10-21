# Detection profiles

A detection profile describes one market: what is sold, which agents scan which sources, how leads are scored, which contacts to prefer and how the LLM qualifies a lead.

## Storage

Profiles live in Postgres. Each edit creates a new revision; earlier revisions are kept. The API validates every definition before storing it. Profiles are never read from a file at runtime. The files under `docs/examples/profiles/` are payloads for the profile API and for tests.

## Schema

The JSON Schema is `docs/schemas/profile.v3.schema.json`. It is generated from `ProfileDefinition` and must not be edited by hand. Regenerate it with:

```
UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples
```

Without `UPDATE_SCHEMA=1` the same test fails if the committed file is stale. The schema describes the shape only. Further rules (regular expressions compile, version extractors have a capture group, no duplicate ids, unique agent ids, at least one agent) are enforced by `ValidatedProfile::parse`.

## Examples

- `docs/examples/profiles/iam-ferriskey.json`: sells FerrisKey and Autharie to organizations running Keycloak or a SaaS IAM. Uses certificate transparency, HTTP probes, CNAME patterns, public tenders and two job agents of the same kind (`ats-fr`, `ats-en`), the second with its own relevance criteria.
- `docs/examples/profiles/ecommerce-replatforming.json`: sells Magento migrations. Uses one HTTP probe with a version extractor, BOAMP and France Travail. It has no IAM-specific field.

## Sections

- `schema_version`: must be 3. See [Older versions](#older-versions).
- `identity`: the offer and the target customer, in free text.
- `agents`: the detection agents; at least one is required. An agent searches one source with its own settings (see ADR 0005). Several agents of the same kind are allowed.
  - `id`: slug matching `^[a-z0-9][a-z0-9-]{0,62}$`, unique in the profile.
  - `name`: display name, not empty.
  - `enabled`: defaults to `true`. A disabled agent is kept but not run.
  - `source`: an object whose `kind` selects the source; the other fields are that kind's settings (table below).
  - `llm.relevance_criteria`: optional, not empty when present. Criteria for triaging this agent's candidates; the profile's criteria apply without it. Qualification of a lead always uses the profile's criteria.
- `scoring`: weighted factors, each made of a signal, an optional value and a weight. The weight is an integer from 1 to 10, validated by the server (`scoring.factors[i].weight`). A lead's score is the sum, over factors, of `weight × measure`. For a normal factor the measure is the highest strength (0 to 1) among the lead's signals of that kind, and of that value when one is set; several matching signals count once. The reserved factor `distinct_signals` measures the number of distinct signal kinds on the lead. The score and its per-factor breakdown are recomputed whenever the lead's signals change, and stored on the lead. Scores stay in points.

  The maximum possible score is the sum, over factors, of `weight × cap`. The cap is 1 for a normal factor; for `distinct_signals` it is the number of distinct signal kinds among the profile's other factors (values are ignored: two `product_detected` factors count one kind). `ProfileDefinition::max_score()` computes it. Example: the IAM example has weights 10, 5, 3, 8, 7, 2 on six kinds plus `distinct_signals` at 2, so its maximum is 35 + 2 × 6 = 47.- `contact_ranking`: preferred email local parts, most preferred first.
- `llm`: the qualification threshold, the relevance criteria and the answer language given to the model. `language` (`llm.language`, `fr` or `en`, default `fr` when absent) is the language of every free-text field the LLM writes: triage reason, qualification summary, approach angle, draft subject and body. The rendered system prompt states it in the market context block (`Write every free-text field (...) in French. Evidence quotes must be copied verbatim from the documents, in their original language.`), so evidence quotes stay in the source language and the verbatim check is unchanged. Changing the language changes the system prompt, hence the LLM cache key: answers are regenerated. Profiles without the field answer in French without a new revision. `qualification_threshold` is a percentage, an integer from 0 to 100 (`llm.qualification_threshold`), of the maximum possible score. A lead is sent to qualification when its score is at least `threshold / 100 × max_score` (`ProfileDefinition::qualification_points()`). With the IAM example, 40 % of 47 is 18.8 points. Changing a weight moves the points a lead needs, not the percentage.

## How scoring works

Each factor looks for one signal kind, optionally one value. Its contribution is `weight × strength` of the strongest matching signal on the lead (strength from 0 to 1, set by the source). No matching signal gives 0. The `distinct_signals` factor contributes `weight × number of distinct signal kinds` on the lead, which rewards leads seen by several agents.

- Lead score = sum of the contributions, in points.
- Max score = sum of the weights, with `distinct_signals` counted as `weight × number of distinct signal kinds among the other factors`.
- A lead is sent to qualification when `score >= threshold / 100 × max score`. Above it, the LLM writes an assessment and a draft for human approval.

Strengths set by the sources:

| Origin | Signal | Strength |
| --- | --- | --- |
| HTTP probe | `product_detected` | 0.9 |
| HTTP probe | `debt_marker` | 0.6 |
| DNS CNAME | `saas_provider` | 0.9 |
| Keyword candidate accepted by the rules | keyword sources (`public_tender`, `job_offer`, ...) | 0.7 |
| Candidate accepted by the LLM triage | keyword sources | 0.6 |
| Address found in `security.txt` | `role_contact_found` | 0.7 |
| Address extracted by the LLM | `role_contact_found` | 0.5 |
| Address guessed from a role | `role_contact_found` | 0.2 |

Example with the IAM weights: a lead with a Keycloak fingerprint (strength 0.9) and a job offer accepted by the rules (strength 0.7) scores `10 × 0.9 + 8 × 0.7 = 14.6` points. With a threshold of 40 % of a maximum of 47, the bar is 18.8 points, so this lead is not qualified until more signals arrive.

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

## Run a source now

Sources come in two modes (ADR 0006).

- **Continuous** sources listen around the clock inside the binary: `certificate_transparency`, `dns_cname`, `http_probe`, `security_txt`, `public_page` and `role_guess`. They have no recurring job and nothing to run on demand: an edited agent is picked up on the listener's next loop (a few seconds when idle), and a new certificate or lead flows through the pipeline within minutes. A listener is active while at least one enabled agent of its kind exists in an active profile (contact finders are always active); otherwise it is "Inactif (aucun agent)". With several replicas one holds each listener at a time (Postgres advisory lock); the others show "En veille (autre instance)" and take over when it stops.
- **Scheduled** sources (`boamp`, `decp`, `ted`, `france_travail`, `ats`, `llm_qualification`) run on a recurring job, so an edited agent otherwise waits for the next interval. "Lancer maintenant" on the Sources screen (Configuration > Sources), or "Lancer la source" on an agent card, moves the pending job to the present.

The Sources screen shows continuous sources in their own "Écoute continue" group with the listener state, last activity, backlog (tiles of lag for certificate transparency, hosts to probe, contact targets), totals and last error. Agent cards of continuous kinds show an "En écoute" status line instead of the run button.

- `GET /api/v1/sources` lists every known source. `mode` is `continuous` or `scheduled`. A continuous source carries a `listener` object (`state`, `holder`, `last_activity_at`, `last_error`, `items_total`, `signals_total`, `backlog`) and `status` mirrors the listener state; a scheduled source carries its job state, and one without a job, for example `france_travail` without credentials, has `status: "unavailable"`. Both list their latest runs; a continuous source records a run only for a loop that processed at least one item.
- `POST /api/v1/sources/{source}/run` answers 202 with the updated job. It answers 409 when the source is already running or listens continuously, 404 for an unknown source or one without a job, and 403 for service accounts. `llm_qualification` is the qualification job.

For a scheduled source the worker picks the job up on its next poll. The run covers every active profile with an enabled agent of that kind, using the saved revisions only.

A holder refreshes its status every `LISTENER_IDLE_SECONDS`, even in the middle of a long step. A listener silent for longer than `max(3 × LISTENER_IDLE_SECONDS, 120 s)` is reported `stale` ("Sans nouvelles"): its instance no longer answers and another replica takes over.

A host or domain whose lookup, probe or crawl fails is marked checked like a success, so it waits for the normal revisit interval instead of being retried on every loop. Timeouts and rate limits retry after one hour.

`LISTENER_IDLE_SECONDS` (default 15) is the sleep after a loop that found nothing; `LISTENER_LOCK_RETRY_SECONDS` (default 15) is how often a standby replica retries the lock.

## Older versions

Stored revisions are never rewritten. Versions 1 and 2 are accepted and upgraded when read; new revisions are written in version 3. Errors on an older document keep that document's paths.

### Version 1 to 2

Version 1 listed sources as optional fields of a `sources` object. Parsing upgrades it to one enabled agent per configured source, in this order: certificate transparency, HTTP probe, DNS CNAME, BOAMP, DECP, TED, France Travail, ATS. The agent `id` is the kind in kebab case (`certificate-transparency`, `france-travail`) and the `name` is its display name.

### Version 2 to 3

Versions 1 and 2 had free number weights and a threshold in points. The upgrade, applied to both:

- Weights: when every weight is an integer from 1 to 10 they are kept. Otherwise each becomes `max(1, round(w × 10 / largest weight))`.
- Threshold: `clamp(round(old threshold / old maximum × 100), 0, 100)`, with the old maximum computed from the old weights by the same formula. When the old maximum is not positive the threshold becomes 50.

For the IAM v1 and v2 examples the weights 30, 15, 10, 25, 20, 5, 5 become 10, 5, 3, 8, 7, 2, 2, and a threshold of 50 points out of 135 becomes 37 %.
