# 0004. Documented public APIs skip robots.txt

Status: accepted
Date: 2026-10-05

## Context

The BOAMP source (#45) reads open public tenders from the OpenDataSoft Explore API (`https://boamp-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/boamp/records`). The dataset is published by DILA under the Licence Ouverte and listed on data.gouv.fr as "API BOAMP". It needs no key and returns quota headers (`x-ratelimit-limit` 2 000 000 per day). The robots.txt of that host disallows `/api/` for every agent except Googlebot, so a robots check would make the documented endpoint unusable by design.

robots.txt governs crawling and indexing of web pages. A documented machine interface published for programmatic reuse is not a crawl target.

## Decision

Requests made with `FetchPurpose::Api` do not consult robots.txt. `FetchPurpose::Crawl` still does, and refuses a disallowed path. Everything else applies to API requests unchanged: public-target check, per-host and per-source rate limiting, redirect policy, body cap and the identifiable `User-Agent` with a contact.

Only a source whose endpoint is a documented public API may use `FetchPurpose::Api`. Sources allowed to:

- BOAMP (OpenDataSoft Explore API on `boamp-datadila.opendatasoft.com`)
- Certificate Transparency monitoring endpoints (static-ct-api, ADR 0003)

Adding a source to this list requires amending this ADR.

## Consequences

- BOAMP can be read without scraping the HTML site.
- The exemption is per request kind and cannot be applied to arbitrary web pages by accident: pages fetched from company sites stay `Crawl` or `Probe`.
- The rate limit stays the only brake on API traffic, so each such source must keep a quota and respect upstream quota headers.
- If the publisher later asks for crawlers to stay away from the API, the source is disabled rather than worked around.
