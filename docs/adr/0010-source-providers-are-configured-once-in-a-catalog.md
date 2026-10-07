# 0010. Source providers are configured once in a catalog

Status: accepted
Date: 2026-10-07

## Context

Sources were wired in the composition root from environment variables (`BOAMP_*`, `FRANCE_TRAVAIL_*`, `TED_*`) and each profile only chose which keywords to run. Credentials lived in the process environment, an operator could not see which sources were available or working, and a wrong client secret only showed up as a failed run. Providers with an account (France Travail today, an ATS or a company registry tomorrow) need credentials, sometimes an OAuth2 consent, and a way to check them before agents depend on them.

## Decision

Lodestone offers a **catalog of source providers**. Each provider is configured once on a dedicated page and becomes usable by sourcing agents only after its connection has been tested successfully.

### Catalog in code

The catalog is code, not data. A `SourceProviderDescriptor` declares a provider's `key`, name, description, category (public procurement, jobs and missions, technical signals, companies), authentication mode, configuration fields and capabilities (keyword search, filters for remote work, countries and contract type), plus a note about the terms of use. A field has a key, a French label, a kind (`text`, `url`, `secret`, `select`, `number`, `bool`), a required flag, a default and a help text. Adding a provider is a descriptor and a probe, with no migration. Market-specific criteria (keywords, filters, weights) stay in detection profiles, never in a descriptor.

### One configuration per provider, organisation-wide

Lodestone is single-tenant (ADR 0001): there is one `ProviderConfiguration` per provider key, shared by every profile. It stores the validated settings (JSON), the encrypted secrets, the status, the time and message of the last test and, for the authorization code mode, the connected account. A provider with no row is `not_configured`.

### Authentication modes

- **none**: nothing to configure beyond optional settings.
- **API key**: the key is a secret field; the engine injects it in the header the descriptor names, with its prefix.
- **OAuth2 client credentials** (service account): client id (a setting) and client secret (a secret). The engine fetches an access token, keeps it in an in-memory cache and renews it when less than a safety margin (60 seconds) remains. The cache is per process and lost on restart; nothing about it is persisted.
- **OAuth2 authorization code** (an operator connects an account): `POST .../oauth/start` creates a `state` and a PKCE verifier (S256), stores them server-side bound to the user with a ten-minute expiry, and returns the authorization URL. The provider redirects the browser to `GET /api/v1/source-providers/oauth/callback`, which is the only unauthenticated route of the feature, because a browser redirect carries no bearer token: the single-use state, bound to the user who started the flow, authorises it. The callback validates the state (unknown, replayed or expired states are refused before any call to the provider), exchanges the code, stores the access and refresh tokens encrypted, records the connected account and redirects the browser to the provider page of the webapp with `?connected=1` or `?error=<code>`. An expired access token is refreshed on use under a lock, a refresh refused by the provider marks the provider `failing`. Disconnecting revokes the token when the descriptor knows a revocation endpoint (best effort), then clears the tokens and the account.

The redirect URI registered at the provider is `SERVER_PUBLIC_URL` followed by `/api/v1/source-providers/oauth/callback`; the browser lands on `SERVER_WEBAPP_URL`.

### Test gating

Each provider has a cheap authenticated probe, a `ProviderProbe` selected by provider key through an enum (static dispatch). Saving a configuration (`PUT`) stores it as `untested` and runs the probe; the status becomes `ready` only on success and `failing` otherwise, keeping the message for the operator. A new test (`POST .../test`) can be run at any time. A provider in the authorization code mode with no connected account stays `untested`; the callback runs the test once the account is connected. `is_provider_usable(key)` is true only for `ready`, and is what sourcing agents and runs must call before using a provider. Changing the settings or a secret resets the status, so a stale `ready` never survives a different configuration.

### Secrets

Secrets are encrypted with the key that already protects LLM provider keys (`LLM_SECRETS_ENCRYPTION_KEY`, AES-256-GCM), bound to the provider key and secret name as associated data. The API never returns them: it returns whether each secret is set and a hint (four dots followed by the last four characters when the secret is long enough to show them). A request leaves a secret unchanged by omitting it, replaces it with a string, and removes it with `null`. Secrets, tokens, authorization codes and verifiers are never logged and never appear in a probe message.

### Relation to sourcing agents

Agents use configured providers. Issue 190 delivered the machinery with two providers, `boamp` (no authentication, test = its API answers) and `france_travail` (client credentials, test = token request and one authenticated call). Issue 191 registers every other source: `decp`, `ted`, `ats`, `certificate_transparency`, `dns_cname`, `http_probe`, `contacts` (security.txt, contact pages and role guesses) and `company_research`. All have no authentication; their test is a GET on the public endpoint they use, or a message that there is no external endpoint for the crawler-style ones. A source or listener runs only while its provider is `ready`: a scheduled job completes with a `provider not ready` info log and checks again within five minutes, a listener stays idle, "Lancer maintenant" answers `409`, and the Activity view shows « Provider non prêt » with a link to the catalog page. France Travail reads its client id, secret and token URL from the catalog through the engine's token cache. Providers without authentication are created and tested at every start when missing or not ready; `FRANCE_TRAVAIL_*` variables only seed the France Travail configuration on the first start, while no configuration exists. Changing the agent model so an agent selects a provider is issue 192.

## Consequences

- An operator sees every provider, its status and the last test message in one place, and a broken credential is found when it is entered rather than at the next run.
- Credentials leave the process environment for the database, protected by the existing encryption key. Rotating that key requires entering the secrets again; a secret that cannot be decrypted makes the test fail instead of crashing.
- The OAuth callback is reachable without a bearer token. Its protection is the unguessable single-use state with a short expiry; it exposes no data and always answers with a redirect.
- Token caches are per process: with several API replicas each fetches its own client credentials token, and a refresh token that the provider rotates is stored by whichever replica refreshed it.
- Adding a provider requires a descriptor and a probe in code, which keeps field validation, secret handling and the UI form generic.

## Update: agents over providers, URLs read at run time

A `search` agent (schema 16) lists the catalog providers it searches and defines keywords, exclusions and filters once; each provider translates the filters it supports and the rest is applied after collection (`docs/profiles.md`, "Search agents"). Signal agents name their provider. The URL settings of the catalog are read at each run; the environment only seeds them.
