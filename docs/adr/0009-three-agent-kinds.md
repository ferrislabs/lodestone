# 0009. A profile has three agent kinds: sourcing, qualification and scoring

Status: accepted (implemented by issue 172)
Date: 2026-10-07

## Context

ADR 0005 introduced sourcing agents and ADR 0007 qualification agents with an aggregator. In ADR 0007 the verdict is decided in code: the agents' opinion (weighted mean of calibrated question scores) is blended with the evidence strength into a confidence, compared with two thresholds, and capped by rules (single angle, weak evidence, non-production). Two weaknesses remain.

- The final judgement is a formula over the answers of angle-by-angle agents. Nothing in the system actually *weighs* the answers, the evidence and the context together the way a human reviewer does, and an operator cannot choose which model takes that decision, nor what it is asked.
- The evidence indicators were counted per lead. Two domains of one company counted twice, identical signals seen several times counted several times, and a job offer from two years ago weighed as much as one from last week (issue 170).

## Decision

A profile has three kinds of agents.

1. **Sourcing agents** (ADR 0005) extract leads from one source. They never judge relevance beyond the triage rules of their source.
2. **Qualification agents** (ADR 0007) evaluate each lead from one angle with a JSON contract: per question the chosen option, a confidence, a probability per option and a verified quote. They may exclude a lead. Their output is evidence for the next step, not the decision.
3. **Scoring agents** are LLM agents that *decide* the score and the verdict. They answer a **normed contract imposed by the application**, in the style of the Laya and Jev classifiers: criteria choices with probabilities, a score from 0 to 100, probabilities for each verdict and reasons. The operator chooses the provider and the prompt of a scoring agent, never the shape of its answer. A profile may declare several scoring agents, each with a weight.

Aggregation of the scoring agents:

- The **lead score** is the weighted mean of the agents' scores.
- The **verdict** is the argmax of the weighted mean of the agents' verdict probabilities.
- **Qualification exclusions remain a hard rule.** A lead excluded by a qualification agent is `ecarte` whatever the scoring agents answer (ADR 0007, "Exclusions").
- The code-computed **opinion × evidence** (ADR 0007) stays, shown on the lead page as the "Indicateurs" and given to the scoring agents as documents, but it no longer decides the verdict.
- The **aggregator** (ADR 0007) is kept: it writes the summary and the message draft once the other agents have answered.

Order of a lead through the pipeline: sourcing, qualification agents (with exclusions), indicators computed in code, scoring agents, aggregator.

### Supersession

The scoring agents are implemented in issue 172 and the verdict part of ADR 0007 ("Evidence and calibration": confidence blend, thresholds `reliable_from` and `discard_below`, caps) is superseded by this decision: the blend, the thresholds and the caps are still computed, but only as the "Indicateurs"; they no longer decide. Exclusions, the contract of qualification agents, the aggregator and the company dossier are not superseded.

## Scoring agents (issue 172)

Profile schema 11 adds `scoring_agents`. The contract, the inputs, the combination, the storage and the due logic are specified in `docs/profiles.md` ("Scoring agents"). The decisions:

- **Imposed contract.** `criteria` (same shape and grounding as qualification answers, no exclusion), an integer `score`, a `verdict` with `choice`, `confidence` and probabilities for `fiable`, `a_verifier` and `ecarte`, and 1 to 5 `reasons`. Validated like a qualification answer, one retry, an invalid answer makes the agent result missing.
- **Combination.** Weighted mean of the scores, rounded; argmax of the weighted mean of the verdict probabilities with prudent ties (`a_verifier`, then `ecarte`, then `fiable`); the confidence of the verdict is the probability of the winner. Qualification exclusions force `ecarte` with confidence 100. No valid scoring result: verdict `null` and reason `awaiting_scoring`. `calibrate_verdict` is the identity until a learned calibration applies to the agent (issue 171, "Learned calibration" in `docs/profiles.md`).
- **Pipeline.** qualification agents (escalation included), scoring agents, aggregator. A lead is due for scoring when its qualification results changed or its profile revision changed; the aggregator waits for the scoring of the lead.
- **Rescoring.** The loop without model call refreshes the indicators and the decay and no longer decides; it recombines stored scoring results, and only exclusions can change a verdict.
- **Migration.** A lead keeps the verdict computed by the former formula (`verdict_source` `indicators`) until its first scoring.
- **Quality.** Snapshots keep the scoring results; the versions table counts precision per scoring agent and per version with a `kind`.

## Indicators given to the scoring agents (issue 170)

The indicators are computed in code without any model call and must be reliable inputs, so issue 170 changes how they are counted. Implemented before the scoring agents exist, they already feed the ADR 0007 confidence.

**One entity per company.** A company is identified by its SIREN, resolved by the company dossier (ADR 0007, "Company dossier"). Table `companies` holds one row per SIREN (legal name from the dossier, latest build wins). `company_dossiers.company_siren` references it and is set whenever a dossier holds official data. Leads are linked through their domain's dossier: there is no column on `leads`, two leads are siblings when their domains' dossiers point at the same SIREN. The view `lead_siblings` lists the pairs of siblings of the same profile, suppressed domains excluded. A lead whose domain has no official data stays standalone.

**Aggregation.** Facts and evidence of a lead aggregate its own signals, sources, signal kinds and hosts with those of its siblings *of the same profile only*. Siblings in other profiles are shown on the lead page but never counted: another profile is another market with its own scoring. The lead's own signal score stays its own.

**Independence.** In the evidence volume, signals with an identical kind, normalised value (trimmed, lowercased) and source count once, across the lead and its siblings. The freshest observation of a group stands for it. Diversity counts distinct sources and distinct signal kinds.

**Decay.** Each independent signal weighs `0.5 ^ (age in days / half-life of its kind)`; the half-life comes from `evidence.half_life_days` in the profile. See `docs/profiles.md` for the formulas and the defaults.

**Rescoring.** `SCORING_VERSION` is 3, so every stored lead is rescored from its stored answers on the next loop, without calling a model. A lead is also rescored when the number of signals it aggregates changes, including a signal recorded on a sibling domain or a dossier that links two leads.

## Consequences

- Three agent kinds make the editor, the API and the documentation say who does what: sourcing finds, qualification examines, scoring decides.
- The scoring contract is imposed, so a profile cannot break the arithmetic of the aggregation, and small classifiers can fill it.
- A company with several domains is one entity in evidence and in the list (grouped rows), while each domain keeps its own lead, status and decisions.
- Decay is computed when a lead is scored; between two rescorings the stored evidence ages. A lead that nothing touches keeps the strength it had at its last scoring.
- The decision is taken by the scoring agents; the ADR 0007 formula, fed by the corrected indicators, is kept as the indicators they are given.

## Amendment: no signal weights (#180)

With the scoring agents deciding the score and the verdict, the weighted `scoring.factors` of the profile only computed `leads.score`. Profile schema 13 removes `scoring` and `evidence.reference_score`; the upgrade from 12 discards them. The strength of the signals comes only from the sources, which already give each signal a `strength` from 0 to 1.

- **Signal strength.** The mean of the three highest `strength x decay` among the independent signals of the lead and its siblings of the same profile (fewer when there are fewer, 0 when none). It is the `strength` component of the evidence (replacing `signal score x freshness / reference score`), and `leads.score` = round(strength x 100, 1).
- **Where it is written.** At each recording of signals (sources, contact finder), with the clock of the run, and by the rescoring loop for every lead it rescores, since decay changes it with the days.
- **Staleness.** A lead is stale when its scoring version is older, its stored evidence is older than a day or missing, or the number of signals it aggregates changed. The comparison of the stored signal score with `leads.score` is gone: the score is a function of the signals and of the date, refreshed with them.
- **Public meaning.** `min_score`, the score sorts, the `score` field of exports and webhooks and the lead page show the signal strength from 0 to 100, labelled "Force des signaux" in the webapp. `score_breakdown` is removed.
