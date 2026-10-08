# 0011. Sourcing finds leads, enrichment completes them

Status: accepted (implemented by issue 202)
Date: 2026-10-08

## Context

Profile schema 16 (ADR 0010) has two kinds of sourcing agents: `search` agents over keyword providers and `signal` agents over a technical provider (`certificate_transparency`, `dns_cname`, `http_probe`). Three things mixed finding a company with learning more about it.

- The HTTP probe only probed hosts found in certificates or imported by an operator. A lead found in a public tender or a job board was never probed, although its domain was known.
- Career pages (`ats`) were a keyword provider of search agents, yet they are not a keyword search: they read the offers of a company whose domain is already known, and only for companies detected by the technical sources (`PASSIVE_SOURCES`).
- A hostname seen in a certificate only became a candidate host. The lead appeared only if the probe or the DNS lookup matched, so the certificate itself was never evidence.

Qualification started as soon as a lead had a signal, so the agents often judged a lead before the probe or the job offers had a chance to add evidence.

## Decision

A profile separates **sourcing**, which finds leads, from **enrichment**, which completes every lead whatever found it.

### Sourcing agents

`agents[]` has three types (profile schema 17):

- `search`: keywords, exclusions, filters and `accept_min_keywords` over catalog providers that support keyword search (BOAMP, DECP, TED, France Travail, Hacker News). `ats` is no longer one of them.
- `certificates`: hostname `labels`. A certificate host matching an agent's labels creates or updates a lead for its registrable domain with a weak `certificate_seen` signal (value: the host, raw: the matched labels, strength 0.3). The host is still recorded as a candidate host.
- `dns`: CNAME `patterns`, unchanged; a matching target creates or updates a lead.

### Enrichment

A profile section `enrichment` holds two steps run on every lead of the profile.

- `http_probe` (`enabled`, `fingerprints`, `excluded_products`, `extra_subdomains`, default `auth`, `sso`, `login`, `id`, `idp`, `accounts`): the probe covers the registrable domain of each lead, the candidate hosts known for that domain and the extra subdomains. Hosts derived from a lead are candidate hosts with the origin `lead`; the HTTP probe listener seeds them, then probes them with the other due hosts, at the same revisit interval.
- `company_jobs` (`enabled`, `keywords`, `accept_min_keywords`), shown as « Offres d'emploi de l'entreprise »: the career page or the recruiting board of every lead domain of the profiles that enable it is read, whatever the source of the lead. Offers go through the keyword routing with the step's keywords (empty means the union of the enabled search agents' keywords). It keeps its scheduled job and its revisit interval.

Signals produced by enrichment are the same as before (`product_detected`, `debt_marker` from `http_probe`, `job_offer` from `ats`) and carry no agent. **Enrichment never creates a lead**: a match only attaches to the lead that already exists for that domain in the same profile; a match on a domain without a lead in a profile is ignored for that profile and counted as `skipped_no_lead` in the run report. Certificates and DNS are the only technical paths that create leads. Each run of a step is recorded per lead in `lead_enrichments (lead_id, step, ran_at)`: the HTTP probe step ran for a lead once every candidate host of its domain has been probed, the company jobs step once its domain has been scanned.

### Qualification waits for enrichment

A lead is due for qualification once each enabled enrichment step ran for it, or once `ENRICHMENT_WAIT_MINUTES` (default 30, 0 disables the wait) have passed since the lead was created, like the company dossier wait (ADR 0007). In addition, a lead whose only evidence is `certificate_seen` signals (contact signals do not count) is never sent to the models: it waits, with the reason `awaiting_enrichment_evidence` (« En attente d'enrichissement »), until a signal of another kind arrives. The lead API exposes an `enrichment` block with the steps, their last run, the status (`disabled`, `pending`, `complete`, `timed_out`) and the waiting reason.

### Upgrade from schema 16

Stored revisions are upgraded when read.

1. `http_probe` signal agents become `enrichment.http_probe`: fingerprints merged and deduplicated by id, excluded products merged, enabled when one of them was.
2. `ats` is removed from the providers of search agents; `enrichment.company_jobs` is enabled when one of those agents was, with the union of their keywords and the first one's `accept_min_keywords`. A search agent left without provider is dropped.
3. Search agents with identical keywords, exclusions, filters, `accept_min_keywords` and enabled flag are merged into one with the union of their providers. A merged agent is named « Offres et missions » when all its providers are in the jobs and missions category, « Marchés publics » when all are public procurement, else it keeps the first agent's name; its id is the slug of the name, made unique in the profile.
4. Certificate transparency and DNS signal agents become `certificates` and `dns` agents with the same id, name and settings.

A profile may have no sourcing agent when an enrichment step is enabled, so a stored profile made only of an HTTP probe agent stays valid.

## Consequences

- Every lead can collect technical and hiring evidence, including leads from tenders and job boards.
- Certificates create many weak leads; the evidence gate keeps them away from the models until something else confirms them.
- Qualification of a new lead is delayed by up to the enrichment wait.
- The catalog descriptor `ats` becomes « Offres d'emploi de l'entreprise » in the Entreprises category, without keyword search. The webapp editor for the new sections is issue 203.
