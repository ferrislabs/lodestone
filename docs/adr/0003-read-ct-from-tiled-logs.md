# 0003. Read Certificate Transparency from tiled logs directly

Status: proposed
Date: 2026-10-04

## Context

The CT source (#30) must see new certificates continuously, filter hostnames on profile labels at ingestion, and survive restarts without reprocessing everything. The runtime is one binary and one Postgres (ADR 0001). Candidates, checked on 2026-10-04:

- Certstream (CaliDog public websocket): repeated outages reported in March 2025, November 2025 and February 2026 ([issues](https://github.com/CaliDog/certstream-server/issues), [#110](https://github.com/CaliDog/certstream-server/issues/110)). Self-hosting means running [certstream-server-go](https://github.com/pki-tools/certstream-server-go) next to Lodestone.
- crt.sh: 5 requests per minute per IP on HTTPS, 5 concurrent connections on its Postgres port, frequent 429/502 under load ([crt.sh group](https://groups.google.com/g/crtsh/c/NZJntKrBdmg)). Query by domain only, no stream.
- Direct log reading. Chrome's log list (`https://www.gstatic.com/ct/log_list/v3/log_list.json`) has per operator a `logs` array (RFC 6962, 11 usable) and a `tiled_logs` array with `monitoring_url`, `state`, `temporal_interval`. Tiled logs follow [static-ct-api](https://c2sp.org/static-ct-api): `<monitoring>/checkpoint` (tree size, a few seconds of cache), immutable data tiles `<monitoring>/tile/data/<N>` of 256 entries, issuers by fingerprint. Let's Encrypt writes only to tiled logs since 2026-02-28. Measured: Let's Encrypt Sycamore 2026h2 grew from 991 909 995 to 991 911 719 entries in about 60 s, about 29 entries per second for that one log.

## Decision

Implement `CtSource` (the trait from #30) first as a tiled log reader: load the usable and qualified tiled logs from Chrome's log list, keep a cursor (last processed index) per log in Postgres, poll each checkpoint, fetch new full data tiles through the polite client (one host per log operator, rate limited), parse leaves, extract DNS names from the certificate or precertificate, filter labels against the union of active profile labels, and upsert matching hosts into `candidate_hosts` with origin `certificate_transparency`. Start a new log at its current tree size, not at 0. Only full tiles are read, so an entry is seen at most one tile late (256 entries). The trait stays open for an RFC 6962 `get-entries` reader and a crt.sh lookup by domain (backfill for a known domain), not built now.

## Alternatives rejected

- Certstream public instance: unreliable, and a third party decides what we see.
- Self-hosted certstream-server-go: a second process to deploy and monitor, against the one-binary constraint.
- crt.sh as the main feed: no stream, 5 requests per minute, overloaded. Kept only as a possible per-domain lookup.
- RFC 6962 logs only: the largest issuer no longer writes there.

## Consequences

- Gains: no third-party aggregator, data straight from the logs, restart-safe through cursors, static immutable tiles that cache and retry well.
- Costs: parsing tile leaves and X.509 (new dependency for certificate parsing, to choose in #30), tracking log list changes (temporal shards every half year).
- What becomes harder: full-ecosystem coverage. Volume across all tiled logs is several hundred entries per second; #30 must measure throughput and may start with a subset of logs.
- Known limit: wildcard certificates (`*.example.com`) hide the subdomain labels, so `sso.example.com` behind a wildcard is invisible to the label filter.
