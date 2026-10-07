# Detection profiles

A detection profile describes one market: what is sold, which agents scan which sources, which qualification agents judge a lead, which scoring agents decide its score and verdict, how an aggregator sums it up, how signals are weighted and which contacts to prefer.

## Storage

Profiles live in Postgres. Each edit creates a new revision; earlier revisions are kept. The API validates every definition before storing it. Profiles are never read from a file at runtime. The files under `docs/examples/profiles/` are payloads for the profile API and for tests.

## Schema

The JSON Schema is `docs/schemas/profile.v11.schema.json`. It is generated from `ProfileDefinition` and must not be edited by hand. Regenerate it with:

```
UPDATE_SCHEMA=1 cargo test -p lodestone-core --test profile_examples
```

Without `UPDATE_SCHEMA=1` the same test fails if the committed file is stale. The schema describes the shape only. Further rules (regular expressions compile, version extractors have a capture group, no duplicate ids, unique agent ids, at least one agent) are enforced by `ValidatedProfile::parse`.

## Examples

- `docs/examples/profiles/iam-ferriskey.json`: sells FerrisKey and Autharie to organizations running Keycloak or a SaaS IAM. Uses certificate transparency, HTTP probes, CNAME patterns, public tenders and two job agents of the same kind (`ats-fr`, `ats-en`), the second with its own relevance criteria.
- `docs/examples/profiles/ecommerce-replatforming.json`: sells Magento migrations. Uses one HTTP probe with a version extractor, BOAMP and France Travail. It has no IAM-specific field.

## Sections

- `schema_version`: must be 11. See [Older versions](#older-versions).
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
  - `description`: what the agent evaluates, 1 to 300 characters. Shown on the agent card and on the lead page.
  - `provider_id`: id of a declared LLM provider (`GET /api/v1/llm/providers`), or `null` for the default provider. The reference is not checked by profile validation: the runtime resolves it and treats a missing or disabled provider as an error.
  - `system_prompt`: not empty, at most 8000 characters.
  - `weight`: integer from 1 to 10, default 5, the agent's weight in the lead confidence.
  - `exclusion_threshold`: from 0.5 to 1, default 0.6, the probability at which an excluding option excludes the lead (see "Exclusions").
  - `samples`: integer from 1 to 5, default 3, the number of independent model calls per lead (see "Sampling and voting").
  - `sampling_temperature`: from 0 to 1.5, default 0.7, the temperature of those calls when `samples` is above 1; it overrides the provider temperature for them.
  - `questions`: at least one. A question has an `id` (lowercase letters, digits, `-`, `_`, at most 63 characters, unique in the agent), a `wording`, a `weight` (1 to 10, default 5) and 2 to 11 `options`. An option has a `value` (same identifier rule, unique in the question), a `label`, a `score` from 0 to 1 `is_unknown` (default false) and `excludes` (default false, never together with `is_unknown`). Exactly one option per question is the unknown option (`is_unknown: true`, default value `inconnu`, label `Inconnu`, score 0.5): the model picks it when nothing in the documents supports an answer. Its score is editable, it cannot be removed, and the editor pins it last as `Option « inconnu »`. A question without exactly one unknown option is rejected (`UnknownOptionCount`, path `...options`).
  - Validation paths include the indexes, for example `qualification_agents[1].questions[0].options[2].score`.
  - Ready-made templates (editor, "Ajouter un agent de qualification"; French prompts and labels, higher score = better lead for the offer; the operator adjusts scores and weights):
    - `Pertinence`: axis `pertinence` (`forte`, `moyenne`, `faible`).
    - `Maturité technique`: axis `maturite` (`legacy_auto_heberge`, `saas`, `inconnu`).
    - `Environnement`: production or non-production evidence. Axes `environnement` (`production`, `mixte`, `hors_production`, `inconnu`) and `maturite_usage` (`usage_reel`, `experimentation`, `inconnu`). Reads the "Faits" block (environment per host, production share).
    - `Type et taille d'organisation`: axes `type` (`secteur_public`, `grand_compte`, `eti`, `pme`, `startup`, `inconnu`) and `taille` (`grande`, `moyenne`, `petite`, `inconnue`). Scores are generic.
    - `Marché`: the prompt starts with an editable sentence, `Zone visée : France et Europe francophone.`. Axes `zone` (`dans_la_zone`, `proche`, `hors_zone`, `inconnu`) and `langue` (`francais`, `autre`, `inconnu`). Reads the country by ccTLD from the "Faits" block.
    - `Opportunité`: migration, technical debt, cost or limits of the current SaaS, hiring. Axes `migration` (`en_cours`, `probable`, `improbable`, `inconnu`) and `declencheur` (`fort`, `moyen`, `aucun`).
    - `Recherche entreprise`: what the company does and its size, from its dossier.
    - `Exclusions`: weight 1. Axes `editeur_integrateur`, `hebergeur_agence` (the signal concerns a customer of the company), `domaine_inactif` (`parque_ou_inactif`, `actif`, `inconnu`), `client_actuel` and `concurrent`; the other four have `oui` (excludes, score 0), `non` (score 1) and `inconnu` (score 0.5). The prompt holds three operator-editable lists in French, `Clients actuels :`, `Concurrents :` and `Éditeurs et intégrateurs du domaine :`, with short IAM-oriented defaults.
    - The IAM example (`docs/examples/profiles/iam-ferriskey.json`) enables all of them.
- `scoring_agents`: zero or more (ADR 0009). Each agent *decides* a score and a verdict for every lead from the reports of the qualification agents, through a contract imposed by the application. With none enabled the verdict stays pending (`en_attente`, reason `awaiting_scoring`).
  - `id`: slug, unique among all agents of the profile (sourcing, qualification and scoring). `name`, `enabled` (default `true`), `description` (1 to 300 characters), `provider_id` (`null` for the Synthèse default provider, the one of the aggregator), `system_prompt` (not empty, at most 8000 characters).
  - `weight`: integer from 1 to 10, default 5, the agent's weight in the lead score and verdict.
  - `criteria`: at least one. Same rules as qualification `questions` (identifier, `wording`, `weight`, 2 to 11 options with a score from 0 to 1, exactly one unknown option), except that no option can exclude a lead (`CriterionCannotExclude`, path `scoring_agents[i].criteria[j].options[k].excludes`).
  - `samples`: integer from 1 to 5, default 1. `sampling_temperature`: from 0 to 1.5, default 0.7, used when `samples` is above 1.
  - Default agent given to every profile of an older version on upgrade (and carried by both examples): `score-global`, "Score global", weight 5, 1 sample, provider `null`, a French description and prompt, and three criteria (`adequation_offre` forte/moyenne/faible, `solidite_preuves` solide/partielle/fragile, `priorite_commerciale` haute/normale/basse), each with an unknown option.
- `aggregator`: `provider_id` (or `null` for the synthesis default) and a `system_prompt` (not empty, at most 8000 characters). Once every enabled qualification agent has a result for a lead, it writes the lead summary and a message draft (see "Qualification and summary").
- `escalation`: `provider` (id of a declared LLM provider, `null` by default which disables escalation), `band_low` (0 to 100, default 40) and `band_high` (0 to 100, default 70) with `band_high > band_low` (see "Escalation"). The whole section and each field are optional.
- `verdict`: `reliable_from` (0 to 100, default 70) and `discard_below` (0 to 100, default 30), with `reliable_from > discard_below`. Percentages of the lead confidence: reliable from the first (and only when the two conditions below hold), to check between the two, discarded below the second. `calibration_floor` (0 to 0.2, default 0.05, 0 disables) clamps every answer probability to `[floor, 1 - floor]` before scoring, then renormalises. `min_evidence` (0 to 1, default 0.5) is the evidence strength a lead needs to be reliable.
- `evidence`: how the strength of the evidence weighs on the confidence (ADR 0007, "Evidence and calibration"). `influence` (0 to 1, default 0.6), `reference_score` (at least 1, default 50) and `weights` (`volume`, `diversity`, `environment`, `strength`, integers from 0 to 10, default 1, at least one above 0). `half_life_days` maps a signal kind to its half-life in days (see "Evidence indicators"). The whole section and each field are optional.

### Scoring agents

Order of a lead: sourcing, qualification agents (with exclusions and escalation), indicators computed in code, scoring agents, aggregator. The qualification listener runs the three phases in that order on every pass.

**Contract imposed by the application.** `scoring_contract_schema(&ScoringAgent)` (`libs/core/src/domain/profile/contract.rs`) builds it; the operator edits the prompt and the criteria, never this shape:

```json
{
  "criteria": {
    "<criterion id>": {
      "type": "choice",
      "choice": "<option value>",
      "evidence": "<sentence copied verbatim from the documents>",
      "confidence": 0.0,
      "probabilities": { "<option value>": 0.0 }
    }
  },
  "score": 0,
  "verdict": {
    "choice": "fiable | a_verifier | ecarte",
    "confidence": 0.0,
    "probabilities": { "fiable": 0.0, "a_verifier": 0.0, "ecarte": 0.0 }
  },
  "reasons": ["<1 to 5 sentences of at most 200 characters, in the profile language>"]
}
```

Validation is `validate_scoring` (`libs/core/src/domain/scoring_agent/mod.rs`) and follows the qualification rules: every criterion is checked like a question (choice among the options, confidence and probabilities from 0 to 1, probabilities summing to 1 within 0.05 then renormalised) and grounded against the documents (a quote not found downgrades the criterion to the unknown option, after one retry that lists the ungrounded criteria); `score` is an integer from 0 to 100; `verdict.probabilities` has exactly `fiable`, `a_verifier` and `ecarte`, summing to 1 within 0.05 then renormalised; `reasons` has 1 to 5 non empty strings of at most 200 characters. One retry lists every violation. An answer still invalid after the retry makes the result of that agent invalid (stored with its error, left out of the combination). The stored verdict is the argmax of the probabilities (ties: `a_verifier`, then `ecarte`, then `fiable`) with its probability as confidence; a `verdict.choice` that disagrees is kept as `declared_choice`.

**Inputs.** Documents given to a scoring agent, in order: the lead overview with the facts (company entity, independent and decayed evidence), the signals as sentences, the company dossier, one report per enabled qualification agent (source `qualification:<id>`: description, summary, voted answers with quotes, agreement between samples, ungrounded flags, exclusions raised), and an `indicators` document (agents opinion, evidence strength with its four components, independent signals, decayed weight, indicator confidence). The system prompt is the agent prompt, the market context, the language line, the notes on facts, dossier, reports and indicators, and the contract. When no qualification result is valid, no scoring call is made: every enabled scoring agent gets an invalid result ("aucun rapport de qualification valide") and the verdict stays pending.

**Samples.** With `samples` above 1, each sample is a contract call with its own validation and cache entry. The criteria are voted with the qualification `vote` rules; the score is the mean of the valid samples rounded to the nearest integer; the verdict probabilities are the mean of the valid samples (then renormalised) and the verdict is their argmax; the reasons are those of the first sample that agrees with the final verdict and the most final criteria. Raw samples are kept in `scoring_results.raw_samples`.

**Combination.** Pure function `combine` / `decide` (`libs/core/src/domain/scoring_agent/combination.rs`), over the valid results of the enabled scoring agents:

- lead score = weighted mean of the agent scores (weights 1 to 10), rounded half up to an integer;
- verdict probabilities = weighted mean of the agents' verdict probabilities, each first passed through `calibrate_verdict(probabilities, agent)`, the identity until issue 171;
- verdict = argmax, ties resolved prudently `a_verifier` > `ecarte` > `fiable`; verdict confidence = that probability, stored in percent on the lead (`leads.confidence`);
- qualification exclusions force `ecarte` with reason `excluded` and confidence 100, whatever the scoring agents say, even when none answered;
- no valid scoring result and no exclusion: verdict `null`, score `null`, reason `awaiting_scoring`.

**Storage and due logic.** Results live in `scoring_results` (lead, profile revision, agent, status, criteria answers, score, verdict, reasons, provenance, raw samples), upserted on `(lead_id, profile_revision, agent_id)`. The lead score is `leads.llm_score`, the verdict and its confidence `leads.verdict` and `leads.confidence`, the verdict probabilities, reason and source `lead_qualification_state.verdict_probabilities`, `verdict_reason` and `verdict_source` (`scoring`, or `indicators` for a lead qualified before the scoring agents existed and not scored since: it keeps its former computed verdict until scored). The indicators stay in `opinion`, `evidence`, `indicator_confidence` and `indicator_cap`. A lead is due for scoring when its qualification results changed (`lead_qualification_state.completed_at` differs from `scoring_for`) or its profile revision differs from `scoring_revision`. The aggregator only summarises a lead whose scoring is up to date, and receives the scoring results (score, verdict, reasons) as documents; it still must not restate figures.

**Rescoring.** The rescoring loop (no model call) refreshes the indicators and the decay (`SCORING_VERSION` 4) and no longer decides: it recombines the stored scoring results and changes a verdict only through exclusions (a lead scored by the scoring agents is recombined, an `indicators` lead is untouched unless a qualification agent excludes it).

**Quality.** Decision label snapshots keep the scoring results (`scoring_agents`, `llm_score`, `indicator_confidence`, `verdict_source`); the versions table of `GET /api/v1/quality` has a `kind` (`qualification`, `scoring`, `aggregator`), so precision is counted per scoring agent and per version (model and prompt).

### Evidence indicators

The strength of the evidence is computed in code (ADR 0007, "Evidence and calibration", ADR 0009) from the facts of the lead, aggregated over its company.

**Company.** Leads whose domains have dossiers with the same SIREN belong to one company entity (`companies`, keyed by SIREN, legal name from the dossier). Facts and evidence of a lead aggregate the signals, sources, signal kinds and hosts of the lead and of its siblings of the *same profile*; siblings in other profiles are listed on the lead page (`company.siblings`) but never counted. A lead without a SIREN stays standalone.

**Independence.** Signals with the same kind, normalised value (trimmed, lowercased) and source are one independent signal, across the whole company. `facts.signal_count` counts every signal, `facts.independent_signals` the independent ones. The age of an independent signal is the age of its freshest observation.

**Formulas.** With `n` independent signals, `age_i` the age in days at the instant of scoring (never negative) and `h(kind)` the half-life of the kind:

- `decay_i = 0.5 ^ (age_i / h(kind_i))`, so 1 today, 0.5 after one half-life.
- `decayed = sum(decay_i)`.
- `volume = 1 - exp(-decayed / 3)`.
- `diversity = min(1, (sources + 0.5 x (kinds - 1)) / 2)` with `sources` the distinct sources and `kinds` the distinct signal kinds (at least one kind counts as zero extra).
- `environment`: share of production hosts among the hosts carrying signals, 0.5 when none.
- `strength = clamp(signal_score x (decayed / n) / reference_score, 0, 1)`, the signal score of the lead weighted by the mean freshness of its independent signals (1 when there is no signal).
- `score = weighted mean of the four components` with `evidence.weights`.

**Half-lives.** `evidence.half_life_days` is an object of integers from 1 to 3650, keyed by signal kind (lowercase snake case) plus `default`. Defaults: `job_offer` 90, `public_tender` 180, `awarded_contract` 365, `saas_provider` 365, `product_detected` 365, `debt_marker` 365, `contact` 365, `role_contact_found` 365, `default` 180. A kind absent from the object falls back to its default above, then to `default`, then to 180. The editor shows one field per kind in "Demi-vie des signaux (jours)".

**API.** The lead page shows `facts.independent_signals`, `facts.sibling_domains` and, in `evidence`, `independent_signals`, `decayed_signals`, `source_count` and `kind_count`. The lead list accepts `group_by=company`: one row per company and profile, the lead with the best confidence (then score) standing for the others, which are returned in `grouped_leads`; the other filters apply before grouping, and `total` counts rows. Default is `none`. The webapp groups by default ("Regrouper par entreprise") and shows a "+N domaines" badge that expands the sibling domains.

**Rescoring.** `SCORING_VERSION` 3 (v10 counting rules) rescored every stored lead without calling a model; version 4 (v11) refreshes the indicators of every stored lead the same way. A lead is rescored whenever the number of signals it aggregates changes, including signals of a sibling. Decay is evaluated at each scoring; it does not run on its own.

### Exclusions

An option with `excludes: true` marks a reason to drop the lead (publisher or integrator of the domain, current customer, competitor, parked domain). The rule is `exclusions(&[QualificationResult], &[QualificationAgent])` in `libs/core/src/domain/qualification/exclusion.rs`, a standalone function so that a later scoring agent can leave it as a hard rule over its own verdict. For every enabled agent with a valid result, each axis produces an `Exclusion` when:

- the answer is grounded (a verified quote; a downgraded answer is not grounded) and its quote is not empty, and
- the raw stored probability of an excluding option is at least the agent's `exclusion_threshold`. The raw probability is used, not the calibrated one: calibration clamps probabilities away from 0 and 1 to temper overconfidence in scores, but an exclusion is a hard decision that needs an explicit answer and the calibration would only move a probability a few points. When several excluding options of an axis reach the threshold the most probable one is kept.

An `Exclusion` holds the agent id and name, the axis id, the option value and label, the probability, the quote and the document it was found in. Any exclusion sets the verdict to `ecarte` with `verdict_reason` `excluded`, whatever the confidence; the confidence is still computed and shown. Exclusions are stored with the lead qualification state (`lead_qualification_state.exclusions`, JSON), applied by evaluation and by rescoring (`SCORING_VERSION` 2 rescored every stored lead), exposed as `exclusions` on the lead, and counted per agent and option by `GET /api/v1/quality` (`exclusions`). The prompt marks excluding options in the option list: `exclusion: choose only with explicit evidence`.

The quality report also lists the most recent leads rejected for `deja_client`, `concurrent`, `editeur_integrateur` or `hebergeur_agence` (`exclusion_candidates`); the Qualité page copies their legal name and domain so the operator can paste them into the lists of the Exclusions agent prompt. No profile is ever changed automatically.

### Sampling and voting

Each (lead, agent) pair runs `samples` independent contract calls. Every call has its own validation, retry and grounding (see "Contract JSON"), its own cache entry (the cache key includes the sample index, and the sampling temperature when one applies, so samples differ and a replay is served from the cache) and is charged to the run budget. With more than one sample, every call uses `sampling_temperature`; with one sample the provider temperature applies and nothing below happens. A sample that stays invalid after its retry is dropped; the result is invalid only when every sample is invalid, and then carries the first sample error. A budget exhausted in the middle of an agent leaves the lead due, as before.

The voting is the pure function `vote(agent, samples)` in `libs/core/src/domain/qualification/voting.rs`, over a list of validated outputs (post-downgrade) so that other agents can reuse it. Per axis:

- the final choice is the majority (the most voted option) of the valid samples; a tie goes to the option with the higher mean reported probability, then to the option declared first;
- the probabilities are `0.5 × vote share + 0.5 × mean of the reported probabilities` per option, then renormalised (`VOTE_SHARE_WEIGHT` is 0.5): the vote share keeps a lone sure answer from dominating, the reported probabilities keep the nuance;
- `agreement` is the share of valid samples that chose the final option, stored in the answer;
- `confidence` is the mean confidence of the samples that chose the final option;
- the evidence and its source are those of the first grounded sample that chose the final option (else the first sample that chose it); the answer is grounded when any sample that chose the final option was grounded, and a non grounded answer keeps the `original_choice` of that first sample;
- the summary is the one of the first sample that agrees with the most final choices.

A single valid sample (a one-sample agent, or only one left) is kept as it is; in the second case its agreement is 1. The result stores `provenance.samples` (the number of valid samples that voted), the generation settings with the effective temperature, and `raw_samples`, the JSON of every sample (its summary and answers, or its error) for audit; it is empty for a one-sample result. The lead page shows "Accord : 2/3" per axis. Exclusions (see below) and rescoring read the voted answers.

### Escalation

After a lead's confidence is computed, when `escalation.provider` is set and the confidence is in `[band_low, band_high)`, every enabled agent of the lead is run again with the escalation provider, one sample, temperature 0 (its own cache entries). The results that come back valid replace the originals (`escalated: true`, with the escalation provider in the provenance); an agent whose escalated call is invalid keeps its original result. The confidence and verdict are then computed again from the replaced results, then the aggregator runs as usual. A lead is escalated at most once per evaluation. Each time its agents re-run because it became due again (a signal from a new source, new dossier content, a new profile revision) the escalation is decided afresh, so a lead still in the grey band is escalated again. A replay of the same evaluation (a crash before the results are stored, a re-run on cached answers) hits the cache for both the agents and the escalation, so it makes no new call. `lead_qualification_state.escalated_revision` records the revision of the last evaluation that escalated the lead. An escalation that exhausts the run budget leaves the lead due and a replay reuses the cached calls. The lead page marks an escalated result "Escaladé vers <fournisseur>" and the Qualité page counts escalated runs on their own rows of the versions table.

### Contract JSON

Grounding (issue 167). The `evidence` of every answer is checked against the documents given to that agent (lead facts, signal sentences, company dossier) with the same typographic normalisation as the aggregator's evidence. A choice other than the unknown option whose quote is empty or not found is retried once, listing the ungrounded questions. If it is still ungrounded the answer is downgraded, never the whole result: `choice` becomes the unknown option, `probabilities` puts 1 on it (the other options 0, then calibration applies as usual), `grounded` is `false` and `original_choice` keeps the first choice for display. The stored answer keeps `evidence`, `evidence_source` (source of the first document containing the quote), `grounded` and `original_choice`; rescoring reads the downgraded answer. A quote longer than 300 characters is cut to 300 before the check. The lead page shows the quote in italics with its source and, for a downgraded answer, the badge "Non étayé — ramené à « Inconnu »" with the original choice. The aggregator document of an agent lists the quote of each answer and an `ungrounded:` line for a downgraded one.

A qualification agent answers, and only answers, with one object per question:

```json
{
  "answers": {
    "<question id>": {
      "type": "choice",
      "choice": "<option value>",
      "evidence": "<sentence copied verbatim from the documents>",
      "confidence": 0.0,
      "probabilities": { "<option value>": 0.0 }
    }
  }
}
```

`contract_schema(&QualificationAgent)` (`libs/core/src/domain/profile/contract.rs`) builds the JSON Schema of this contract from the agent's questions: every question required, `choice` limited to the option values, `confidence` and every probability a number from 0 to 1, `probabilities` holding exactly the option keys, no additional property. Contract v3 adds `evidence`, a required string of at most 300 characters: the sentence of the documents that justifies the choice, copied verbatim. It may be empty only when the choice is the unknown option. The editor shows it read-only as "Aperçu du JSON contractuel".

- `scoring`: weighted factors, each made of a signal, an optional value and a weight. The weight is an integer from 1 to 10, validated by the server (`scoring.factors[i].weight`). A lead's score is the sum, over factors, of `weight × measure`. For a normal factor the measure is the highest strength (0 to 1) among the lead's signals of that kind, and of that value when one is set; several matching signals count once. The reserved factor `distinct_signals` measures the number of distinct signal kinds on the lead. The score and its per-factor breakdown are recomputed whenever the lead's signals change, and stored on the lead. Scores stay in points.

  The maximum possible score is the sum, over factors, of `weight × cap`. The cap is 1 for a normal factor; for `distinct_signals` it is the number of distinct signal kinds among the profile's other factors (values are ignored: two `product_detected` factors count one kind). `ProfileDefinition::max_score()` computes it. Example: the IAM example has weights 10, 5, 3, 8, 7, 2 on six kinds plus `distinct_signals` at 2, so its maximum is 35 + 2 × 6 = 47.- `contact_ranking`: preferred email local parts, most preferred first.
- `llm`: the qualification threshold, the relevance criteria and the answer language given to the model. `language` (`llm.language`, `fr` or `en`, default `fr` when absent) is the language of every free-text field the LLM writes: the aggregator's summary, verdict narrative, approach angle, draft subject and body. The aggregator's system prompt states it (`Write every free-text field (...) in French. Evidence quotes stay in the language of the document they come from.`), so evidence quotes stay in the source language and the verbatim check is unchanged. Changing the language changes the system prompt, hence the LLM cache key: answers are regenerated. Profiles without the field answer in French without a new revision. `qualification_threshold` is a percentage, an integer from 0 to 100 (`llm.qualification_threshold`), of the maximum possible score. It is kept for older profiles and no longer gates anything: since ADR 0007 every lead with a signal goes through the qualification agents, whatever its score.

## Qualification and summary

Qualification runs in the `qualification` listener (ADR 0006, ADR 0007), not in a recurring job: the former `llm.qualify` job is gone, its row is deleted by a migration and a stale one is rejected as a permanent failure.

1. **Agents.** Every lead with a signal is evaluated by each enabled qualification agent. The step is repeated when the lead receives a signal from a source it had no signal from, and for every lead when the profile gets a new revision. The lead's confidence (0 to 100) and verdict come from the valid results only, computed in code: calibrated question score, agent score, the weighted mean over agents (the agents' opinion), blended with the evidence strength, then compared with `verdict.reliable_from` and `verdict.discard_below`; a lead is reliable only with two independent angles and enough evidence (see ADR 0007, "Evidence and calibration"). Verdicts: `fiable`, `a_verifier`, `ecarte`; a lead without any valid result has no verdict and is shown "En attente".
2. **Aggregator.** A lead is summarised when the agents' step has stored a result for every enabled agent (valid or invalid) and its summary is missing or older than the agents' results (the `summarised_at` mark of `lead_qualification_state` is reset each time the agents run again). A decided lead (validated, rejected, contacted) is not summarised again. The selection runs in the same listener loop, right after the agents' step, oldest first, within the same batch size and run budget.
3. **Provider.** `aggregator.provider_id`, else the default provider "Synthèse (agrégateur par défaut)". Qualification agents without a provider use "Classification (agents de qualification par défaut)". The two defaults keep the former tier names in the database and the API (`triage` and `qualification`); the Fournisseurs LLM screen shows them under "Fournisseurs par défaut".
4. **Prompt.** System: the aggregator's `system_prompt`, the market context (offer and target customer, neutralised), the answer language and the output rules. Documents (untrusted, numbered): one per signal first, then the lead overview, then one per enabled agent with its questions, the chosen option label, its confidence and its score (an invalid agent is listed as such and left out of the confidence). The instruction states the computed confidence and verdict as final facts.
5. **Output.** `summary`, `verdict_narrative`, `approach_angle`, `evidence` (quotes with the index of a signal document) and `draft_message` (`subject`, `body`). The output has no confidence or verdict field and any extra field is ignored: the lead's confidence and verdict are never written by the aggregator.
6. **Evidence.** Each quote must appear verbatim, after typographic normalisation, in the signal document it names. An unverified answer is retried once with a reminder. If it is still unverified, the summary is kept with `evidence_verified = false`, only the verified quotes are stored and the lead page and the validation queue show "Preuves non vérifiées". An answer that breaks the output contract twice stores nothing and the lead stays due.
7. **Draft.** Every summarised lead gets a message draft in the validation queue, pending review, whatever its verdict. The lead moves from `new` to `qualified`. Nothing is ever sent. A new summary replaces the previous draft in the queue only while that draft is pending and unedited. A draft a human edited, or already approved or rejected, is carried over unchanged into the new assessment; an edited pending draft also gets the new model text as a read-only suggestion (`suggested_subject`, `suggested_body`) and `outdated_at`, and the review screen says "Le bilan a été mis à jour après votre modification".
8. **Backoff.** A failed aggregation (provider unavailable, call failure, answer breaking the contract twice) is counted per lead in `lead_qualification_state` (`summary_attempts`, `summary_next_attempt_at`, `summary_error`). The next try waits 1 min, 5 min, 30 min, 2 h, then 6 h; from 5 consecutive failures the lead page shows "Bilan en échec : <raison>" and the aggregator keeps trying every 6 h. A success, or new agent results, resets the counter. `POST /api/v1/leads/{lead_id}/summary/retry` (users only) resets it at once; the lead page has a "Réessayer" button. The agents' step has no per-lead loop (a failed call stores an invalid result); when a step exhausts its run budget the listener pauses that step with the same schedule.

The assessment is stored in `assessments` (`prompt_version` is `aggregator@` plus a hash of the system prompt, `verdict_narrative`, `evidence_verified`).

The leads list (`GET /api/v1/profiles/{profile_id}/leads`) accepts `verdict` (comma-separated `fiable`, `a_verifier`, `ecarte`, `en_attente`), `min_confidence` and `sort` (`confidence_desc`, the default, `confidence_asc`, `score_desc`, ...); each lead carries `confidence`, `verdict`, `opinion` (agents' opinion in percent), `evidence` (strength in percent with its four components, signal count, signal score, `only_non_production`), `verdict_reason` (`single_angle`, `weak_evidence` or `non_production` when a confident lead was capped at `a_verifier`) and the latest `summary`. The Leads screen hides `ecarte` by default and sorts by confidence. The queue (`GET /api/v1/drafts`) is sorted by lead confidence and filtered by the same `verdict` parameter; the Brouillons screen also hides `ecarte` by default.

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

Stored revisions are never rewritten. Versions 1, 2, 3, 4, 5, 6, 7, 8, 9 and 10 are accepted and upgraded when read; new revisions are written in version 11. Errors on an older document keep that document's paths.

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

### Version 4 to 5

Qualification agents gain a required `description`. Parsing derives it from the name (`Évalue : <name>`, cut at 300 characters). The agent contract also changes (ADR 0007): the answer carries a required `summary` next to `answers`.

### Version 5 to 6

The upgrade adds the `evidence` section with its defaults and the `verdict.calibration_floor` (0.05) and `verdict.min_evidence` (0.5) defaults; thresholds and everything else are kept. Stored leads are rescored from their stored answers on the next qualification loop, without calling a model.

### Version 6 to 7

Every question gains an unknown option (`is_unknown`). The upgrade marks the first option whose value is `inconnu`, `inconnue` or `unknown`; when there is none it appends `{ "value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true }`, so a question with 10 options upgrades to 11, the new maximum. The contract becomes v3 (answers carry `evidence`). Stored answers without `evidence` stay readable and are rescored unchanged.

### Version 7 to 8

Every qualification agent gains `exclusion_threshold` (0.6) and every option gains `excludes` (false). Stored leads are rescored from their stored answers without a model call (`SCORING_VERSION` 2); no lead is excluded until an option is marked.

### Version 8 to 9

Every qualification agent gains `samples` (3) and `sampling_temperature` (0.7), and the profile gains `escalation` (`provider` null, `band_low` 40, `band_high` 70). An upgraded profile therefore runs three samples per agent, which triples the calls of a lead; set `samples` to 1 to keep the former cost. Escalation stays disabled until a provider is chosen.

### Version 9 to 10

`evidence` gains `half_life_days` with the defaults above. Stored leads are rescored without a model call (`SCORING_VERSION` 3): evidence now counts independent signals, decays with age and aggregates the sibling domains of a company, so strengths can change. The same upgrade applies to older versions, whose `evidence` section is filled with the defaults.

### Version 10 to 11

The profile gains `scoring_agents` with one agent, `score-global` (see `scoring_agents` above), provider `null` (the Synthèse default), weight 5, 1 sample. Stored leads keep the verdict they had, computed by the former confidence formula (`verdict_source` `indicators`), until the scoring agents have scored them: every stored lead becomes due for scoring on the next pass, and is summarised again afterwards because the aggregator now receives the scoring results. Rescoring (`SCORING_VERSION` 4) refreshes the indicators of every stored lead without calling a model. Profiles written in version 11 may declare no scoring agent; their leads then wait with a pending verdict.
