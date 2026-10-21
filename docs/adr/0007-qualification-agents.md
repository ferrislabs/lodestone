# 0007. Leads are qualified by qualification agents and summed up by an aggregator

Status: accepted
Date: 2026-10-06

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
- lead confidence = Σ (agent weight × agent score) / Σ agent weights over valid results, in percent.

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
