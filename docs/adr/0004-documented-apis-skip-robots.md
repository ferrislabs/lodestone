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
- TED (Tenders Electronic Daily Search API, `api.ted.europa.eu/v3/notices/search`, POST with a JSON query, no key; published by the Publications Office of the EU, its robots.txt answers 404)
- Certificate Transparency monitoring endpoints (static-ct-api, ADR 0003)
- DECP (#46), three documented public interfaces:
  - the data.gouv.fr dataset API and the monthly consolidated files it lists (`www.data.gouv.fr/api/1/datasets/...`, then `static.data.gouv.fr`), published by the Ministères économiques et financiers under the Licence Ouverte
  - the Annuaire de l'administration OpenDataSoft API (`api-lannuaire.service-public.fr/api/explore/v2.1/...`), whose robots.txt disallows `/api/` like BOAMP, used to resolve a buyer SIRET to its public website
  - the Recherche d'entreprises API (`recherche-entreprises.api.gouv.fr`, documented, 7 requests per second), used to resolve a SIRET to a company name, and by the company dossier (ADR 0007) to read the official data of a SIREN found on a company's own legal notice (legal name, activity, headcount range, head office, creation date); the SIREN is never guessed from a name
- France Travail (#48), the Offres d'emploi v2 API (`api.francetravail.io/partenaire/offresdemploi/v2/offres/search`, GET with a bearer token obtained by the OAuth2 client credentials grant on `entreprise.francetravail.fr/connexion/oauth2/access_token`). It is a partner API that requires credentials registered by the operator; only fields returned by the API are used and the `urlOrigine` of an offer (often a job board) is never fetched
- Hacker News "Ask HN: Who is hiring?" (#193), through the official HN search API (`hn.algolia.com/api/v1/`: `search_by_date` lists the monthly threads of the `whoishiring` account, `search` returns the comments of one thread matching a keyword). The data is published by Y Combinator under the MIT-licensed API terms, no key, no stated rate limit; the shared per-host limit applies and the collection runs once a day. Only the comment text, author, date and item id are used, and the item URL on `news.ycombinator.com` is the provenance
- ATS job boards (#49), three documented public job board APIs, read for a lead only after its own careers page linked to the board (a slug is never guessed from the domain, and the careers pages themselves stay `Crawl`):
  - Greenhouse Job Board API (`boards-api.greenhouse.io/v1/boards/<token>/jobs?content=true`, GET, no key; its robots.txt only disallows `/embed/`)
  - Lever Postings API (`api.lever.co/v0/postings/<site>?mode=json`, and `api.eu.lever.co` for EU tenants, GET, no key; its robots.txt allows `/` with a crawl delay of 1 second, covered by the per-host rate limit)
  - Ashby public job posting API (`api.ashbyhq.com/posting-api/job-board/<name>?includeCompensation=false`, GET, no key; its robots.txt answers 401)

Adding a source to this list requires amending this ADR.

## Consequences

- BOAMP can be read without scraping the HTML site.
- The exemption is per request kind and cannot be applied to arbitrary web pages by accident: pages fetched from company sites stay `Crawl` or `Probe`.
- The rate limit stays the only brake on API traffic, so each such source must keep a quota and respect upstream quota headers.
- If the publisher later asks for crawlers to stay away from the API, the source is disabled rather than worked around.
