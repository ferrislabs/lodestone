# 0009. A profile has three agent kinds: sourcing, qualification and scoring

Status: accepted
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

The scoring agents are implemented in issue 172. Until that issue lands, ADR 0007 and issue 155 remain in force unchanged. Once it lands, the verdict part of ADR 0007 ("Evidence and calibration": confidence blend, thresholds `reliable_from` and `discard_below`, caps) is superseded by this decision; exclusions, the contract of qualification agents, the aggregator and the company dossier are not.

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
- Until issue 172, the decision is still the ADR 0007 formula, fed by the corrected indicators.
