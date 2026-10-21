# 0008. Human decisions are recorded as labels with the qualification snapshot

Status: accepted
Date: 2026-10-06

## Context

A lead carries a confidence, a verdict and per-agent scores (ADR 0007). Those values are overwritten whenever the lead is re-qualified or re-scored, and a human decision (validate or reject a lead, approve or reject a draft) only changes a status. Nothing tells whether a "fiable" verdict was right, which agent separates good leads from bad ones, or whether the thresholds of a profile fit its market.

## Decision

Every human decision writes a **decision label** in the table `decision_labels`, in the same transaction as the decision.

- Kinds: `lead_validated`, `lead_rejected`, `draft_approved`, `draft_rejected`. A lead moving to validated or rejected, from the lead page or from a draft decision, writes the lead label. A draft decision also writes its draft label. Moving a lead back to `new`, or to `contacted`, writes nothing.
- A label stores the lead, the profile, the profile revision, the user, the instant and a JSON **snapshot** of the qualification at decision time: confidence, opinion, evidence (score and components), verdict, verdict reason and, per agent id, the status and score. The snapshot is a copy: later re-scoring never changes it.
- There is no backfill: the history of qualification states was not kept.

Metrics (`domain/quality/metrics.rs`) are pure functions over labels:

- Lead decisions count once per lead: the latest lead label wins (a lead validated and later rejected counts as rejected). Draft labels are counted separately.
- Precision of a verdict is validated over validated plus rejected among the leads that carried that verdict when decided.
- Per agent: mean score of validated and of rejected leads (valid answers only), their difference and both sample sizes.
- Confidence distribution of validated and rejected leads in 10 buckets of 10 %.
- Share of discarded leads: leads with an "ecarte" verdict over leads with any verdict, by qualification date.
- Threshold suggestions, per profile and only from 20 decided leads that carry a confidence: `reliable_from` is the step-5 value maximising the F1 of "validated" (the highest on ties); `discard_below` is the largest step-5 value under which at most 5 % of the decided leads were validated, kept at least 5 points under `reliable_from`.

The API exposes them at `GET /api/v1/quality` (users only) and the webapp page "Qualité" displays them.

## Consequences

- Suggestions are advice. Lodestone never edits a profile on its own: an operator reads them and changes the thresholds in the Qualification tab, which creates a revision like any edit.
- Labels are deleted with their lead or profile (`ON DELETE CASCADE`); metrics ignore leads of suppressed domains.
- Precision measured on validated leads only reflects what humans decided to look at; leads nobody reviewed carry no label.

## Amendment 2026-10-06: provenance of results and reasons for rejections

### Context

Precision per verdict says nothing about which model or prompt produced it. Changing a prompt, a model or a generation setting could not be judged against human decisions, and a rejection carried no reason, so "the lead was bad" could not be told from "the data was bad".

### Decision

**Run provenance.** Every qualification result and every assessment stores a `RunProvenance` (`domain/llm/provenance.rs`):

- `provider_id` and `provider_name` (the name is a copy: the provider may later be renamed or deleted), `model` (the model that answered, as reported by the provider), `prompt_version` (`qualification-agent@<hash of the prompt>` for an agent, `aggregator@<hash>` for the aggregator), `generation` (temperature, max tokens as effectively sent, frequency, presence and repeat penalties, reasoning effort) and `samples`.
- `samples` is 1 until self-consistency sampling fills it.
- Storage is in columns: `qualification_results` and `assessments` gain `provider_name`, `generation` (JSONB) and `samples`; `assessments` also gains `provider_id`; `qualification_results.prompt_hash` is renamed `prompt_version` and prefixed with `qualification-agent@`. `model`, `provider_id` and `prompt_version` stay plain columns so they can be filtered and grouped in SQL. Rows written before this change read back with an empty provider name, no generation settings and one sample.
- A result with no resolvable provider keeps the requested provider id, an empty model and the prompt version.
- A decision snapshot gains the provenance of each agent (`agents.<id>.provenance`) and of the latest assessment of the lead (`aggregator`). Both are optional: labels recorded before this change have neither and are left out of the metrics by version.

**Metrics by version.** `versions` in `GET /api/v1/quality` gives the precision per (agent, model, prompt version) for the agents that answered validly, and per (model, prompt version) for the aggregator (no agent id). It is computed over the same decided leads as the other metrics. `agents[].name` is the agent name found in the profile revisions, the id when no revision declares it.

**Rejection reasons.** Rejecting a lead (`PATCH /api/v1/leads/{id}/status` with `status: rejected`) or a draft (`POST /api/v1/drafts/{id}/reject`) requires a `reason`, one of `hors_cible`, `deja_client`, `concurrent`, `editeur_integrateur`, `hebergeur_agence`, `mauvaise_donnee`, `autre`, and accepts a `note` of at most 500 characters, required for `autre`. A missing or unknown reason, or a bad note, is a 422 with the field path. The reason and note are stored on the label (`rejection_reason`, `rejection_note`); a draft rejection writes them on both its labels, and the note is also kept as the draft's rejection reason. Reasons are not required to move a lead anywhere else and are rejected by the database on approval labels. `profiles[].rejection_reasons` counts the reasons of the rejected leads of each profile (latest lead decision wins, labels without a reason are not counted).

### Consequences

- Any change of prompt, model or generation setting shows as a new row in "Précision par version" once leads qualified with it are decided. Precision stays comparable only when the sample sizes are.
- Existing callers of the two rejection endpoints must send a reason.
- Reasons are the raw material for exclusion rules (hosting providers, competitors, existing customers); nothing turns them into rules yet.
