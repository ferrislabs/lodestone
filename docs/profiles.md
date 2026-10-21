# Detection profiles

A detection profile describes one market: what is sold, which agents scan which sources, which qualification agents judge a lead and how an aggregator sums it up, how leads are scored and which contacts to prefer.

## Storage

Profiles live in Postgres. Each edit creates a new revision; earlier revisions are kept. The API validates every definition before storing it. Profiles are never read from a file at runtime. The files under `docs/examples/profiles/` are payloads for the profile API and for tests.

## Schema

The JSON Schema is `docs/schemas/profile.v4.schema.json`. It is generated from `ProfileDefinition` and must not be edited by hand. Regenerate it with:

```
UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples
```

Without `UPDATE_SCHEMA=1` the same test fails if the committed file is stale. The schema describes the shape only. Further rules (regular expressions compile, version extractors have a capture group, no duplicate ids, unique agent ids, at least one agent) are enforced by `ValidatedProfile::parse`.

## Examples

- `docs/examples/profiles/iam-ferriskey.json`: sells FerrisKey and Autharie to organizations running Keycloak or a SaaS IAM. Uses certificate transparency, HTTP probes, CNAME patterns, public tenders and two job agents of the same kind (`ats-fr`, `ats-en`), the second with its own relevance criteria.
- `docs/examples/profiles/ecommerce-replatforming.json`: sells Magento migrations. Uses one HTTP probe with a version extractor, BOAMP and France Travail. It has no IAM-specific field.

## Sections

- `schema_version`: must be 4. See [Older versions](#older-versions).
- `identity`: the offer and the target customer, in free text.
- `agents`: the detection agents; at least one is required. An agent searches one source with its own settings (see ADR 0005). Several agents of the same kind are allowed.
  - `id`: slug matching `^[a-z0-9][a-z0-9-]{0,62}$`, unique in the profile.
  - `name`: display name, not empty.
  - `enabled`: defaults to `true`. A disabled agent is kept but not run.
  - `source`: an object whose `kind` selects the source; the other fields are that kind's settings (table below).
  - `llm.relevance_criteria`: optional, not empty when present. Criteria for triaging this agent's candidates; the profile's criteria apply without it. Qualification of a lead always uses the profile's criteria.
- `qualification_agents`: at least one (ADR 0007). Each agent evaluates every lead of the profile, from every source, from one angle.
  - `id`: slug, unique among all agents of the profile, sourcing and qualification together.
  - `name`: display name, not empty. `enabled`: defaults to `true`.
  - `provider_id`: id of a declared LLM provider (`GET /api/v1/llm/providers`), or `null` for the default provider. The reference is not checked by profile validation: the runtime resolves it and treats a missing or disabled provider as an error.
  - `system_prompt`: not empty, at most 8000 characters.
  - `weight`: integer from 1 to 10, default 5, the agent's weight in the lead confidence.
  - `questions`: at least one. A question has an `id` (lowercase letters, digits, `-`, `_`, at most 63 characters, unique in the agent), a `wording`, a `weight` (1 to 10, default 5) and 2 to 10 `options`. An option has a `value` (same identifier rule, unique in the question), a `label` and a `score` from 0 to 1.
  - Validation paths include the indexes, for example `qualification_agents[1].questions[0].options[2].score`.
- `aggregator`: `provider_id` (or `null` for the synthesis default) and a `system_prompt` (not empty, at most 8000 characters). Once every enabled qualification agent has a result for a lead, it writes the lead summary and a message draft (see "Qualification and summary").
- `verdict`: `reliable_from` (0 to 100, default 70) and `discard_below` (0 to 100, default 30), with `reliable_from > discard_below`. Percentages of the lead confidence: reliable from the first, to check between the two, discarded below the second.

### Contract JSON

A qualification agent answers, and only answers, with one object per question:

```json
{
  "answers": {
    "<question id>": {
      "type": "choice",
      "choice": "<option value>",
      "confidence": 0.0,
      "probabilities": { "<option value>": 0.0 }
    }
  }
}
```

`contract_schema(&QualificationAgent)` (`libs/core/src/domain/profile/contract.rs`) builds the JSON Schema of this contract from the agent's questions: every question required, `choice` limited to the option values, `confidence` and every probability a number from 0 to 1, `probabilities` holding exactly the option keys, no additional property. The editor shows it read-only as "Aperçu du JSON contractuel".

- `scoring`: weighted factors, each made of a signal, an optional value and a weight. The weight is an integer from 1 to 10, validated by the server (`scoring.factors[i].weight`). A lead's score is the sum, over factors, of `weight × measure`. For a normal factor the measure is the highest strength (0 to 1) among the lead's signals of that kind, and of that value when one is set; several matching signals count once. The reserved factor `distinct_signals` measures the number of distinct signal kinds on the lead. The score and its per-factor breakdown are recomputed whenever the lead's signals change, and stored on the lead. Scores stay in points.

  The maximum possible score is the sum, over factors, of `weight × cap`. The cap is 1 for a normal factor; for `distinct_signals` it is the number of distinct signal kinds among the profile's other factors (values are ignored: two `product_detected` factors count one kind). `ProfileDefinition::max_score()` computes it. Example: the IAM example has weights 10, 5, 3, 8, 7, 2 on six kinds plus `distinct_signals` at 2, so its maximum is 35 + 2 × 6 = 47.- `contact_ranking`: preferred email local parts, most preferred first.
- `llm`: the qualification threshold, the relevance criteria and the answer language given to the model. `language` (`llm.language`, `fr` or `en`, default `fr` when absent) is the language of every free-text field the LLM writes: the aggregator's summary, verdict narrative, approach angle, draft subject and body. The aggregator's system prompt states it (`Write every free-text field (...) in French. Evidence quotes stay in the language of the document they come from.`), so evidence quotes stay in the source language and the verbatim check is unchanged. Changing the language changes the system prompt, hence the LLM cache key: answers are regenerated. Profiles without the field answer in French without a new revision. `qualification_threshold` is a percentage, an integer from 0 to 100 (`llm.qualification_threshold`), of the maximum possible score. It is kept for older profiles and no longer gates anything: since ADR 0007 every lead with a signal goes through the qualification agents, whatever its score.

## Qualification and summary

Qualification runs in the `qualification` listener (ADR 0006, ADR 0007), not in a recurring job: the former `llm.qualify` job is gone, its row is deleted by a migration and a stale one is rejected as a permanent failure.

1. **Agents.** Every lead with a signal is evaluated by each enabled qualification agent. The step is repeated when the lead receives a signal from a source it had no signal from, and for every lead when the profile gets a new revision. The lead's confidence (0 to 100) and verdict come from the valid results only, computed in code: question score, agent score, then the weighted mean over agents, compared with `verdict.reliable_from` and `verdict.discard_below`. Verdicts: `fiable`, `a_verifier`, `ecarte`; a lead without any valid result has no verdict and is shown "En attente".
2. **Aggregator.** A lead is summarised when the agents' step has stored a result for every enabled agent (valid or invalid) and its summary is missing or older than the agents' results (the `summarised_at` mark of `lead_qualification_state` is reset each time the agents run again). A decided lead (validated, rejected, contacted) is not summarised again. The selection runs in the same listener loop, right after the agents' step, oldest first, within the same batch size and run budget.
3. **Provider.** `aggregator.provider_id`, else the default provider "Synthèse (agrégateur par défaut)". Qualification agents without a provider use "Classification (agents de qualification par défaut)". The two defaults keep the former tier names in the database and the API (`triage` and `qualification`); the Fournisseurs LLM screen shows them under "Fournisseurs par défaut".
4. **Prompt.** System: the aggregator's `system_prompt`, the market context (offer and target customer, neutralised), the answer language and the output rules. Documents (untrusted, numbered): one per signal first, then the lead overview, then one per enabled agent with its questions, the chosen option label, its confidence and its score (an invalid agent is listed as such and left out of the confidence). The instruction states the computed confidence and verdict as final facts.
5. **Output.** `summary`, `verdict_narrative`, `approach_angle`, `evidence` (quotes with the index of a signal document) and `draft_message` (`subject`, `body`). The output has no confidence or verdict field and any extra field is ignored: the lead's confidence and verdict are never written by the aggregator.
6. **Evidence.** Each quote must appear verbatim, after typographic normalisation, in the signal document it names. An unverified answer is retried once with a reminder. If it is still unverified, the summary is kept with `evidence_verified = false`, only the verified quotes are stored and the lead page and the validation queue show "Preuves non vérifiées". An answer that breaks the output contract twice stores nothing and the lead stays due.
7. **Draft.** Every summarised lead gets a message draft in the validation queue, pending review, whatever its verdict. The lead moves from `new` to `qualified`. Nothing is ever sent. A new summary replaces the previous draft in the queue only while that draft is pending and unedited. A draft a human edited, or already approved or rejected, is carried over unchanged into the new assessment; an edited pending draft also gets the new model text as a read-only suggestion (`suggested_subject`, `suggested_body`) and `outdated_at`, and the review screen says "Le bilan a été mis à jour après votre modification".
8. **Backoff.** A failed aggregation (provider unavailable, call failure, answer breaking the contract twice) is counted per lead in `lead_qualification_state` (`summary_attempts`, `summary_next_attempt_at`, `summary_error`). The next try waits 1 min, 5 min, 30 min, 2 h, then 6 h; from 5 consecutive failures the lead page shows "Bilan en échec : <raison>" and the aggregator keeps trying every 6 h. A success, or new agent results, resets the counter. `POST /api/v1/leads/{lead_id}/summary/retry` (users only) resets it at once; the lead page has a "Réessayer" button. The agents' step has no per-lead loop (a failed call stores an invalid result); when a step exhausts its run budget the listener pauses that step with the same schedule.

The assessment is stored in `assessments` (`prompt_version` is `aggregator@` plus a hash of the system prompt, `verdict_narrative`, `evidence_verified`).

The leads list (`GET /api/v1/profiles/{profile_id}/leads`) accepts `verdict` (comma-separated `fiable`, `a_verifier`, `ecarte`, `en_attente`), `min_confidence` and `sort` (`confidence_desc`, the default, `confidence_asc`, `score_desc`, ...); each lead carries `confidence`, `verdict` and the latest `summary`. The Leads screen hides `ecarte` by default and sorts by confidence. The queue (`GET /api/v1/drafts`) is sorted by lead confidence and filtered by the same `verdict` parameter; the Brouillons screen also hides `ecarte` by default.

## How scoring works

Each factor looks for one signal kind, optionally one value. Its contribution is `weight × strength` of the strongest matching signal on the lead (strength from 0 to 1, set by the source). No matching signal gives 0. The `distinct_signals` factor contributes `weight × number of distinct signal kinds` on the lead, which rewards leads seen by several agents.

- Lead score = sum of the contributions, in points.
- Max score = sum of the weights, with `distinct_signals` counted as `weight × number of distinct signal kinds among the other factors`.
- The score is a secondary indicator. Qualification no longer depends on it: every lead with a signal is evaluated by the qualification agents, and its confidence and verdict come from their answers (see "Qualification and summary").

Strengths set by the sources:

| Origin | Signal | Strength |
| --- | --- | --- |
| HTTP probe | `product_detected` | 0.9 |
| HTTP probe | `debt_marker` | 0.6 |
| DNS CNAME | `saas_provider` | 0.9 |
| Keyword candidate accepted by the rules | keyword sources (`public_tender`, `job_offer`, ...) | 0.7 |
| Address found in `security.txt` | `role_contact_found` | 0.7 |
| Address extracted by the LLM | `role_contact_found` | 0.5 |
| Address guessed from a role | `role_contact_found` | 0.2 |

Example with the IAM weights: a lead with a Keycloak fingerprint (strength 0.9) and a job offer accepted by the rules (strength 0.7) scores `10 × 0.9 + 8 × 0.7 = 14.6` points.

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
- **Scheduled** sources (`boamp`, `decp`, `ted`, `france_travail`, `ats`) run on a recurring job, so an edited agent otherwise waits for the next interval. "Lancer maintenant" on the Sources screen (Configuration > Sources), or "Lancer la source" on an agent card, moves the pending job to the present.

The Sources screen shows continuous sources in their own "Écoute continue" group with the listener state, last activity, backlog (tiles of lag for certificate transparency, hosts to probe, contact targets), totals and last error. Agent cards of continuous kinds show an "En écoute" status line instead of the run button.

- `GET /api/v1/sources` lists every known source. `mode` is `continuous` or `scheduled`. A continuous source carries a `listener` object (`state`, `holder`, `last_activity_at`, `last_error`, `items_total`, `signals_total`, `backlog`) and `status` mirrors the listener state; a scheduled source carries its job state, and one without a job, for example `france_travail` without credentials, has `status: "unavailable"`. Both list their latest runs; a continuous source records a run only for a loop that processed at least one item.
- `POST /api/v1/sources/{source}/run` answers 202 with the updated job. It answers 409 when the source is already running or listens continuously, 404 for an unknown source or one without a job, and 403 for service accounts.

For a scheduled source the worker picks the job up on its next poll. The run covers every active profile with an enabled agent of that kind, using the saved revisions only.

A holder refreshes its status every `LISTENER_IDLE_SECONDS`, even in the middle of a long step. A listener silent for longer than `max(3 × LISTENER_IDLE_SECONDS, 120 s)` is reported `stale` ("Sans nouvelles"): its instance no longer answers and another replica takes over.

A host or domain whose lookup, probe or crawl fails is marked checked like a success, so it waits for the normal revisit interval instead of being retried on every loop. Timeouts and rate limits retry after one hour.

`LISTENER_IDLE_SECONDS` (default 15) is the sleep after a loop that found nothing; `LISTENER_LOCK_RETRY_SECONDS` (default 15) is how often a standby replica retries the lock.

## Older versions

Stored revisions are never rewritten. Versions 1, 2 and 3 are accepted and upgraded when read; new revisions are written in version 4. Errors on an older document keep that document's paths.

### Version 1 to 2

Version 1 listed sources as optional fields of a `sources` object. Parsing upgrades it to one enabled agent per configured source, in this order: certificate transparency, HTTP probe, DNS CNAME, BOAMP, DECP, TED, France Travail, ATS. The agent `id` is the kind in kebab case (`certificate-transparency`, `france-travail`) and the `name` is its display name.

### Version 2 to 3

Versions 1 and 2 had free number weights and a threshold in points. The upgrade, applied to both:

- Weights: when every weight is an integer from 1 to 10 they are kept. Otherwise each becomes `max(1, round(w × 10 / largest weight))`.
- Threshold: `clamp(round(old threshold / old maximum × 100), 0, 100)`, with the old maximum computed from the old weights by the same formula. When the old maximum is not positive the threshold becomes 50.

For the IAM v1 and v2 examples the weights 30, 15, 10, 25, 20, 5, 5 become 10, 5, 3, 8, 7, 2, 2, and a threshold of 50 points out of 135 becomes 37 %.

### Version 3 to 4

The upgrade adds:

- one qualification agent `pertinence` (name "Pertinence", weight 5, `provider_id` null) whose system prompt is the former triage intent followed by the offer, the target customer and `llm.relevance_criteria` (cut at 8000 characters), with one question `pertinence` ("Ce lead est-il pertinent pour l'offre ?", weight 5) and the options `forte` (1), `moyenne` (0.5), `faible` (0);
- an aggregator with a null provider and a default English system prompt; the answer language comes from `llm.language` at runtime;
- `verdict` with 70 and 30.

Sourcing agents, scoring, contact ranking and `llm` are unchanged. A null `provider_id` means the default provider, resolved at runtime.
