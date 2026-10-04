# IAM profile HTTP fixtures

Real responses captured on 2026-10-04 from official images run locally (no third-party host was queried). One JSON file per request; `index.json` lists them with image, digest, path and status.
Container addresses are rewritten to `https://sso.example.test`. `Date`, `Set-Cookie` and request id headers are dropped. Keys and JWKS come from throwaway instances. The upstream error-reporting DSN in the Authentik config response is redacted.
Used by the HTTP probe tests (#27) to check the `iam-ferriskey` example profile's fingerprints.

## Findings

### Keycloak (26.4.7 start-dev, 24.0 start-dev, 17.0.1-legacy, 26.4 --http-relative-path=/auth)
- 26.4 and 24.0 answer: /realms/master 200, /realms/master/.well-known/openid-configuration 200, / 302 -> /admin/.
  /auth/realms/master 404 with body {"error":"Unable to find matching target resource method"} (26.4), /health* 404 (needs KC_HEALTH_ENABLED, mgmt port).
- /realms/master body is JSON with exactly keys realm, public_key, token-service, account-service, tokens-not-before; token-service ends /protocol/openid-connect, account-service ends /account.
- OIDC discovery: 56 keys on 26.4, includes mtls_endpoint_aliases, backchannel_authentication_endpoint, authorization_response_iss_parameter_supported; issuer = <base>/realms/master.
- Headers on realm endpoints: strict-transport-security, x-content-type-options, x-frame-options: SAMEORIGIN, referrer-policy: no-referrer, cache-control: no-cache; 24.0 also x-xss-protection. No server header.
- Version: not exposed unauthenticated in any capture (no header, no body field).
- Legacy 17.0.1: /auth/realms/master 200 (same 5 keys, token-service starts /auth/realms/master/...), /realms/master 404 html "<html><head><title>Error</title></head><body>404 - Not Found</body></html>",
  /auth/ 200 html "<title>Welcome to Keycloak" (WildFly), / 200 static page (Red Hat license comment, last-modified header), discovery 50 keys, has mtls_endpoint_aliases.
- Legacy vs current: path prefix of token-service/account-service/issuer (/auth/realms/... vs /realms/...), and which of /realms/master or /auth/realms/master returns 200.
  Current Keycloak with --http-relative-path=/auth is identical to legacy by path: /auth/realms/master 200, /realms/master 404 (json error body in 26, html in 17), / 302 Location: /auth,
  /auth/ 302 -> /auth/admin/. Differences vs 17 legacy: 26 has content-length+json 404, legacy 404 is text/html; 26 discovery has 56 keys vs 50.

### Authentik (ghcr.io/goauthentik/server:2026.8, 2026.8.3 per image label; no redis needed)
- / 302 Location: /setup (fresh); /-/health/live/ 200 and /-/health/ready/ 200 empty body, header x-powered-by: authentik only (content-length 0).
- /api/v3/root/config/ 200 JSON keys error_reporting, capabilities, cache_timeout, cache_timeout_flows, cache_timeout_policies.
- /api/v3/ 200 HTML (title authentik); /api/v3/admin/version/ 403 JSON.
- Headers on all responses: server: uvicorn, x-powered-by: authentik, content-language, vary: Accept-Language, Cookie (x-authentik-id dropped, request id).
- /application/o/.well-known/openid-configuration 404 and /application/o/ 404 (HTML ~5.5 KB, no providers configured); /realms/master 404 same HTML; /setup 503 at capture time.
- Version: not exposed unauthenticated in captures.

### Zitadel (ghcr.io/zitadel/zitadel:latest = v4.19.4 per logs)
- Needs Host header equal to ExternalDomain (localhost:18191); with 127.0.0.1 host all routes 404 "Instance not found" text.
- /.well-known/openid-configuration 200 JSON (issuer = base without path), also /oauth/v2/keys 200, headers x-robots-tag: none, vary: Origin, Cookie, pragma: no-cache.
- /debug/healthz 200 body "ok", /debug/ready 200, /debug/validate 200, content-type text/plain; charset=utf-8, x-robots-tag: none.
- /ui/console 200 html: <title>ZITADEL • Management Console</title>, <base href="/ui/console/"/>, CSP with 'unsafe-eval', feature-policy: payment 'none'.
- / 302 Location /ui/login body <a href="/ui/login">Found</a>. /realms/master 404 {"code":5,"message":"Not Found"}.
- Version: not exposed unauthenticated in captures.

### FerrisKey (ghcr.io/ferriskey/ferriskey-api:latest, label version "main", openapi info.version 0.9.0)
- Image requires postgres + sqlx migration step (entrypoint sqlx migrate run); realm "master" exists after start.
- /realms/master 401 JSON {"code":"E_UNAUTHORIZED","status":401,"reason":"token_not_found","message":"Token not found"} (differs from Keycloak: realm doc needs a token).
- /realms/<any>/.well-known/openid-configuration 200 JSON, 17 keys, even for nonexistent realm; jwks_uri ends /protocol/openid-connect/jwks.json; no mtls_endpoint_aliases.
- /realms/master/protocol/openid-connect/certs 200 JSON {"keys":[...]}; /api-docs/openapi.json 200 with info.title "FerrisKey API", version "0.9.0" (version exposed unauthenticated, JSON path info.version).
- Headers: vary: origin, access-control-request-method, access-control-request-headers; access-control-allow-credentials: true; access-control-expose-headers: content-disposition; no server header. / and /health 404 empty body.

### Negative
- nginx:alpine: server: nginx/1.31.6; /realms/master 404 html, / 200 "Welcome to nginx!". httpd:alpine: server: Apache/2.4.69 (Unix); / 200 "It works!", /realms/master 404.
