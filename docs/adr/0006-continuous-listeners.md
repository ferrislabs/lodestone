# 0006. Infrastructure and contact sources listen continuously

Status: accepted
Date: 2026-10-05

## Context

Every source ran as a recurring job. Certificate transparency was rescheduled every minute and read a bounded number of tiles per run. DNS CNAME and HTTP probe processed discovered hosts in batches every `JOBS_SOURCE_INTERVAL_MINUTES` (60 by default), so a host seen in a certificate could wait an hour before being probed. Contact finding ran on the same interval.

These sources watch a stream (new certificates, newly discovered hosts, new leads). Waiting for an interval adds latency without saving work, and an operator expects them to run around the clock.

## Decision

Two kinds of sources:

- **Continuous listeners**: certificate transparency, DNS CNAME, HTTP probe, and contact finding (security.txt, public pages, role guess). Each runs as a long-lived task inside the binary: it processes what is due, then sleeps briefly when there is nothing to do, and starts again. Target latency from a new certificate to a signal on a lead: a few minutes.
- **Scheduled sources**: BOAMP, DECP, TED, France Travail, ATS and LLM qualification keep their recurring jobs and can be run on demand (#131).

Listeners are always active while at least one enabled agent of their kind exists in an active profile; contact finding is always active. There is no pause control.

The pipeline is chained through Postgres: certificate transparency stores candidate hosts as due immediately; DNS CNAME and HTTP probe pick due hosts on their next loop; a new lead becomes a due contact target immediately.

With several replicas, each listener runs on exactly one replica at a time: a listener holds a Postgres session-level advisory lock (one key per listener) for as long as it runs. Other replicas retry the lock and take over when the holder stops. This keeps the certificate transparency cursor single-writer and avoids probing a host twice.

Each listener reports its state in a `listeners` table (holder instance, state, last activity, last error, items and signals counters, backlog: certificate transparency lag in tiles, due hosts, due contact targets). The Sources screen shows continuous listeners as "En écoute" with these values instead of a next run date; run-now does not apply to them.

Rate limits, robots rules and the public-target check are unchanged: a listener is a loop around the same source code.

## Consequences

- The recurring jobs of these sources are removed (migration) and no longer scheduled at startup.
- A run row is recorded only for a loop iteration that processed at least one item, so idle loops do not flood `runs`.
- Graceful shutdown stops listeners with the job workers and releases their locks.
- Throughput is bounded by the per-host rate limits; the backlog values make a lag visible.
