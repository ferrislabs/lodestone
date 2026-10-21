# 0007. Leads are qualified by qualification agents and summed up by an aggregator

Status: accepted. The verdict part (confidence blend, thresholds, caps) is superseded by ADR 0009: it is now the indicators given to the scoring agents.
Date: 2026-10-06

Note (2026-10-07): ADR 0009 adds scoring agents that decide the score and the verdict. Once issue 172 lands, the verdict part of this ADR (confidence blend, thresholds and caps, "Evidence and calibration") is superseded; exclusions, the qualification contract, the aggregator and the company dossier stay. Evidence indicators now count independent signals, decay with age and aggregate the sibling domains of a company (issue 170, ADR 0009).

## Context

A profile holds sourcing agents (ADR 0005): each one acts on one source (an HTTP API, the certificate transparency stream, DNS resolution…) and extracts leads, good or bad. After extraction, two fixed LLM steps existed: triage of ambiguous keyword candidates (triage tier) and qualification of a lead (qualification tier: assessment and draft). Both used one global provider per tier, a fixed prompt and a fixed output shape. An operator could not decide which model judges which aspect of a lead, nor what it is asked.

Small local classifiers (Laya, Jev-style models on Ollama) answer classification questions with a fixed JSON contract: for each question, the chosen option, a confidence and a probability per option.

## Decision

A profile has three kinds of agents.

1. **Sourcing agents** (ADR 0005, unchanged): extract leads from a source. Keyword sources keep their deterministic pre-filter (no keyword, no lead); the LLM triage step is removed, every other candidate becomes a lead.
2. **Qualification agents** (new, several per profile): each evaluates **every lead of the profile**, from every source, from one angle. An agent has an id, a name, an enabled flag, a **provider** chosen among the declared LLM providers, a **system prompt**, and a **list of questions**. A question has an id, a wording, a weight and options; each option has a value, a label and a score between 0 and 1. At least one question is required.
3. **One aggregator** per profile: a provider and a system prompt. It reads the results of every qualification agent for a lead and writes the lead's **summary**.

### Contract JSON

A qualification agent must answer, and only answer (contract v2, #152):

```json
{
  "summary": "<2 to 4 sentences, profile language, at most 600 characters>",
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

`summary` is required: 1 to 600 characters once trimmed, written from the documents only, stored with the result and given to the aggregator. Every question is answered; `choice` and the probability keys are option values of that question; confidence and probabilities are between 0 and 1; probabilities sum to 1 within 0.05 and are renormalised. An answer that breaks the contract is retried once, then the agent's result is stored as invalid and counts as missing.

### Scores

Computed in code, never by a model:

- question score = Σ over options of probability × option score;
- agent score = Σ (question weight × question score) / Σ question weights;
- lead confidence = Σ (agent weight × agent score) / Σ agent weights over valid results, in percent (superseded by the amendment on agent weights below).

The lead's verdict comes from two profile thresholds on the confidence: **fiable**, **à vérifier**, **écarté**.

### Summary

The aggregator receives the lead, its signals and every agent's answers and scores. It writes a summary (synthesis, verdict narrative, approach angle, evidence quotes checked verbatim after typographic normalisation) and a **message draft for every lead**. It cannot change the computed confidence or verdict. Drafts follow the validation queue rules: nothing is ever sent.

### When

A lead is qualified when it is created, and again when it receives a signal from a source it had no signal from, or when a qualification agent or the aggregator changes (new profile revision). Qualification runs as a continuous listener (ADR 0006): every due (lead, agent) pair is processed within minutes, then the aggregator runs once all enabled agents have a result for the lead (valid or invalid).

### Providers

Each qualification agent and the aggregator pick their provider. The global triage and qualification tiers are removed. The LLM cache, run budget, reasoning effort and answer language (#139) apply per call.

## Consequences

- Profile schema version 4: `qualification_agents`, `aggregator`, verdict thresholds; v3 is upgraded on read with one default qualification agent (one question "pertinence", options forte/moyenne/faible) using the former triage criteria and the former qualification provider as aggregator.
- Company research (#136) becomes a qualification agent kind that also reads the company site and official data before answering.
- The lead list shows the confidence, the verdict and the summary; the lead page shows every agent's answers.
- Tiers, the triage step and the old qualification job are removed once the new path is in place.

## Implementation notes (#146)

- The aggregator is a second phase of the `qualification` listener, not a separate listener: after the agents' step it picks the leads whose agent results are complete and whose summary is missing or older than those results (`lead_qualification_state.summarised_at`). A failed aggregation leaves the lead due, so the next loop retries it.
- The two defaults keep the `triage` and `qualification` tier values in the database and the API; only their names change in the interface: "Classification (agents de qualification par défaut)" and "Synthèse (agrégateur par défaut)".
- The `llm.qualify` job, the `llm_qualification` source and the fixed prompts (`TRIAGE_V1`, `QUALIFICATION_V1`) are removed. Leads left `uncertain` by the old path are moved back to `new`.
- A summary whose evidence cannot be verified is kept with `evidence_verified = false` instead of marking the lead uncertain; a lead is never blocked from the list.
- Leftover: the per-agent `llm.relevance_criteria` of sourcing agents (profile schema) was meant for the removed triage step and is not read anymore. Dropping it needs a schema version.

## Implementation notes (#152)

- Profile schema version 5: a qualification agent has a `description` (1 to 300 characters, what it evaluates). Version 4 documents are upgraded on read with `Évalue : <name>`; the default Pertinence agent gets a fixed sentence.
- `qualification_results.summary` stores the agent summary (null for results written before the contract v2). The lead detail API returns the agent description and summary; the lead page shows one report card per agent, in profile order, before the aggregator synthesis.
- Each agent block given to the aggregator starts with the agent name, description and summary, then its measurements. In the interface, questions are called "axes d'analyse".

## Implementation notes (#154)

- Lead facts are computed on read by a pure function (`LeadFacts::compute(lead, signals, candidate_hosts)`), never stored: hosts carrying signals with their environment (production, development, staging, test, preproduction, demo, from whole labels or dash-separated tokens of the subdomain such as `dev`, `stg`, `qa`, `pre-prod`; the registrable domain is never inspected), production share, distinct subdomains seen (signals and `candidate_hosts`), signal count, distinct sources, first and last seen, and a probable country from the ccTLD (generic extensions give none).
- The lead detail API exposes `facts` and a French `sentence` per signal. The lead page shows a compact "Faits" block with a warning when every host is non-production.
- The lead overview document given to qualification agents and to the aggregator starts with the facts; the system prompts document the block. Signals are given to the models as French sentences (then source and date) instead of key/value lines and raw JSON; evidence quotes are checked against that same text.

## Amendment: evidence and calibration (#155)

A small model answers `1/0/0` to any question, so one agent with one axis gave a confidence of 100 % and a reliable verdict on a single development CNAME (signal score 12.1). The lead confidence now combines the agents' opinion with the strength of the evidence, and a reliable verdict needs more than one opinion.

All of it is computed in code. Notation: `floor` = `verdict.calibration_floor`, `influence` = `evidence.influence`.

**Calibration.** Before scoring, each answer's probabilities are clamped to `[floor, 1 - floor]` and renormalised to sum to 1 (`floor` default 0.05, 0 disables, at most 0.2). Question and agent scores use the calibrated probabilities. The stored answers keep the raw (already renormalised) probabilities, so a change of floor is applied by rescoring, not by asking the model again.

**Agents' opinion A** (0 to 1): the mean of the agent scores over the valid results (plain mean since the amendment on agent weights below).

**Evidence strength E** (0 to 1), a pure function of the lead facts (#154) and the lead's signal score:

- `volume = 1 - exp(-signal_count / 3)`;
- `diversity = min(1, distinct_sources / 2)`;
- `environment = production_share` (share of production hosts among the hosts carrying signals), 0.5 when no signal names a host. When every host is non-production the share is 0, which already satisfies the "capped at 0.3" rule of the issue, so no separate cap exists;
- `strength = min(1, signal_score / evidence.reference_score)` (default reference 50);
- `E = (wv * volume + wd * diversity + we * environment + ws * strength) / (wv + wd + we + ws)` with the profile weights `evidence.weights` (default 1 each).

Every component is returned by the API and displayed.

**Confidence.** `C = A * ((1 - influence) + influence * E)`, in percent rounded to one decimal, with `influence` in `evidence.influence` (default 0.6). A weak evidence cannot zero a strong opinion (E = 0 keeps 40 % of it) but pulls it well down.

**Verdict.** Discarded below `discard_below`. Reliable needs `C >= reliable_from`, at least two independent angles (two valid agents, or one valid agent with at least two axes) and `E >= verdict.min_evidence` (default 0.5). Otherwise the lead is "to check". When `C` would have been reliable and a rule held it back, the rule is stored as the verdict reason, by priority: `non_production` (evidence below the minimum and every host non-production), `weak_evidence`, `single_angle`.

**Storage.** `lead_qualification_state` gains `scoring_version`, `opinion`, `evidence` (JSON with the components, the signal count and the signal score used) and `verdict_reason`. The lead's `confidence` and `verdict` keep their columns.

**Rescoring.** At the start of every qualification loop, leads of the active revision whose `scoring_version` is older than the code's, or whose stored signal count or signal score differ from the lead's, are rescored from their stored answers: agent scores, opinion, evidence, confidence, verdict and reason are rewritten, with no model call and without touching `qualified_at` or the summary state. The migration sets `scoring_version` to 0, so existing leads are rescored on the first loop after deployment. A new profile revision still requalifies every lead through the agents. A summary written before a rescore keeps its text, which may state the former verdict.

**Luzernrisk.** One `saas_provider=auth0` signal from one source on `login.dev.luzernrisk.com`, signal score 12.1, one agent with one axis answering `forte` 1/0/0: calibrated `A = 0.929`, `E = 0.256` (volume 0.283, diversity 0.5, environment 0, strength 0.242), `C = 51.4 %`, verdict "to check" (before: 100 %, reliable).


## Amendment: synthesis guards and generation parameters (#156)

A small model (gemma4:e2b) repeated "Le verdict est fiable avec une confiance de 100,0 %" five times in the narrative and restated the figures the application already displays.

**Output checks** (pure, `domain/aggregation/checks.rs`), run on every aggregator answer:

- repetition: two sentences of the same field or of two of `summary`, `verdict_narrative`, `approach_angle` that are identical or near-identical after normalisation (case, accents, typography; near = word-set similarity of at least 0.8 for sentences of four words or more). The draft body is checked within itself only;
- length: `summary` 900, `verdict_narrative` 400, `approach_angle` 400, draft subject 120, draft body 2000 characters (constants in the module, also written in the prompt);
- restated figures: in `summary`, `verdict_narrative` and `approach_angle`, any percentage, or a sentence that has a number and a confidence / verdict word (`confiance`, `fiable`, `à vérifier`, `écarté`, and their English counterparts).

A violation triggers the existing single retry, whose message lists the violations (next to the evidence correction when both apply). When the second answer still violates, it is stored with `assessments.quality_warnings` (`repetition`, `too_long`, `restated_figures`) and the lead page shows "Synthèse de qualité incertaine : …". A lead is never blocked. An answer with violations is not cached.

**Prompt.** The model is told that the application displays the confidence and the verdict, that it must not restate them, that `verdict_narrative` explains the reasons in words, that each sentence is written once, and that evidence is copied from the sentence opening a signal document, never from the `source:` / `observed:` lines.

**Evidence flag.** The stored evidence only holds verified quotes, so `evidence_verified` now means "at least one quote was verified" instead of "no quote was dropped". The luzernrisk synthesis showed one verbatim quote flagged "preuves non vérifiées" because another quote of the same answer had been dropped. Dropped quotes are reported as the `dropped_quotes` quality warning. Quotes spanning lines, the `observed:` timestamp and the `dns:` provenance all verify (regression tests).

**Generation parameters.** `llm_providers` gains optional `max_tokens` (1 to 200000, replaces the request's), `temperature` (0 to 2), `frequency_penalty` and `presence_penalty` (-2 to 2) and `repeat_penalty` (1 to 2); absent means not sent. Anthropic sends `max_tokens` and `temperature` only. OpenAI-compatible providers send `temperature`, `frequency_penalty` and `presence_penalty`. Ollama's OpenAI endpoint ignores `repeat_penalty`, both top-level and in `options` (checked against Ollama 0.35 with gemma4:e2b), so it is sent as `frequency_penalty = repeat_penalty - 1` unless a `frequency_penalty` is set.

**Regeneration.** A rescore that changes a lead's verdict marks its summary due again; the aggregator runs again, and edited or decided drafts are protected as for any other new summary.

## Amendment: company dossier (#136)

Company research is not an LLM step. It is a **company dossier** built without any model, then given to every qualification agent and to the aggregator as documents, shown on the lead page, and used by the lead facts. The agents and the aggregator do the summarising; a profile may ship a "Recherche entreprise" agent template that tells an agent to rely on the dossier.

**Scope.** One dossier per registrable domain, shared by the leads of that domain across profiles (table `company_dossiers`). A continuous listener (ADR 0006), `company_research`, builds the dossiers of the domains of leads of active profiles, not suppressed, that have none or whose `next_attempt_at` is reached. Settings: `COMPANY_RESEARCH_MAX_PAGES` (5), `COMPANY_RESEARCH_REFRESH_DAYS` (90), `COMPANY_RESEARCH_WAIT_MINUTES` (10).

**Sources, and nothing else.**

1. The company's own site, fetched with `FetchPurpose::Crawl` (robots.txt, per-host rate limit, public-target check, body cap and redirect policy all apply): the home page, then pages discovered from home page links on the same registrable domain, ranked legal notice (`mentions-legales`, `legal`, `imprint`...), terms (`cgu`, `cgv`), about (`a-propos`, `qui-sommes-nous`, `about`, `societe`, `entreprise`...). When no link of a kind exists, `/mentions-legales`, `/legal`, `/a-propos` and `/about` are tried. At most `COMPANY_RESEARCH_MAX_PAGES` fetches, home page included, tried or not found. Text comes from the existing HTML-to-text, kept up to 8000 characters per page.
2. Official data. A SIREN is read only after a `SIRET`, `SIREN`, `RCS` or `immatriculation` label: 9 digits, or the first 9 of a 14-digit SIRET, and it must pass the Luhn check. It is looked up in the Recherche d'entreprises API (ADR 0004): legal name, NAF code with the label of its section, headcount range, head office city, postcode and country, creation date. No SIREN found means "SIREN introuvable" and no official data: it is never guessed from the name. No search engine, no social network.

**State machine.** `pending` (no build yet, or a refresh was requested on a domain without dossier), then `complete` (site read and official data found), `partial` (site read, no SIREN or no official data) or `unreachable` (the home page is blocked by robots.txt, fails or is empty). A failed build (unreachable site, failed official lookup) backs off along the aggregator schedule (1, 5, 30 minutes, 2, 6 hours), then waits the refresh interval after five attempts; a build without failure waits the refresh interval. A failed refresh keeps the content already collected. A rate-limited fetch defers the domain and never counts as a failure. Failures never block qualification.

**Ordering.** The agents of a lead wait until its dossier is not `pending`, for at most `COMPANY_RESEARCH_WAIT_MINUTES` after `leads.created_at`; then they run without it. A dossier whose content changed (hash of the official data and the pages) gets a new `content_at`; the lead stores the `content_at` it used (`lead_qualification_state.dossier_at`), and a newer one makes its agent results due again, as a signal from a new source does, so the aggregator runs again too. A rebuild with the same content requalifies nothing. With a zero wait there is no gate. (Amended by ADR 0012, "Progressive evaluation": the wait and `COMPANY_RESEARCH_WAIT_MINUTES` are removed, and a new `content_at` marks a debounced, capped requalification instead of the `dossier_at` comparison.)

**Prompts.** Official data lines then each page (text truncated to 3000 characters) are given as documents starting with "Company dossier", after the signal documents; the page URL, or the Recherche d'entreprises URL, is the document source. The aggregator may quote them as evidence, verified like any document; the stored evidence takes the document source as provenance. Both system prompts describe the dossier and tell the model to treat the points as unknown when it is missing.

**Facts.** `LeadFacts` gains `company` (state, whether a SIREN was found, `official`: legal name, SIREN, NAF, headcount range, head office country). A head office country with a known ISO code (France, when the register gives no foreign country) replaces the domain extension for the probable country, and `country_basis` records `head_office` or `tld`. A foreign head office whose country has no ISO code in the register answer keeps the extension. The lead overview document renders both.

**Lead page.** `GET /api/v1/leads/{id}` returns `company`; `POST /api/v1/leads/{id}/company/refresh` (users only) makes the dossier of the lead domain due now. The "Fiche entreprise" block shows the official data, a link to the Annuaire des entreprises, the pages read, the state, the last update and the "Enrichir" button.

## Amendment: signal strength (#180)

`strength` no longer uses a signal score or `evidence.reference_score` (removed in profile schema 13): it is the mean of the three highest `strength x decay` among the independent signals of the lead and its siblings, the strength being the one the source gave each signal. See ADR 0009, "Amendment: no signal weights", and `docs/profiles.md`, "Signal strength".

## Amendment: no qualification agent weights (#184)

The weight of a qualification agent only fed the agents' opinion, an indicator given to the scoring agents for information, and no longer decides the verdict (ADR 0009). Profile schema 15 removes `qualification_agents[].weight`; the upgrade from 14 drops it. The agents' opinion is the plain mean of the calibrated scores of the valid qualification agents, and the prompts no longer print an agent weight. Question weights stay. Rescoring (`SCORING_VERSION` 5) refreshes the indicators without a model call.
