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
