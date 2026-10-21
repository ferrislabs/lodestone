# Writing a detection profile

A detection profile is one JSON document that tells Lodestone what you sell, where to look, how to score what it finds, which contacts to prefer and how the LLM judges a lead. It is stored in Postgres, validated on every write and versioned in revisions. Nothing market-specific lives in code.

This guide is enough to write a profile and import it. Short reference: [profiles](profiles.md). JSON Schema: [`schemas/profile.v1.schema.json`](schemas/profile.v1.schema.json), generated from `libs/core/src/domain/profile/definition.rs` (ADR [0002](adr/0002-profile-definitions-validated-by-rust-types.md)). Rules the schema cannot express are in `libs/core/src/domain/profile/validation.rs`.

Examples you can import as they are:

- [`examples/profiles/iam-ferriskey.json`](examples/profiles/iam-ferriskey.json): IAM, uses every source kind. Walked through below.
- [`examples/profiles/ecommerce-replatforming.json`](examples/profiles/ecommerce-replatforming.json): one probe with a version extractor.
- [`examples/profiles/payroll-saas.json`](examples/profiles/payroll-saas.json): keyword sources only. Second example below.

## How a profile is used

1. Active profiles are loaded by the job workers. A source runs only if at least one active profile enables it (`ProfileDefinition::enables`, `libs/core/src/domain/source/mod.rs`).
2. Sources turn what they find into signals on a lead. A lead is a company, keyed by profile and normalized domain.
3. Each lead gets a score from the profile's `scoring`.
4. Leads with status `new` and a score at or above `llm.qualification_threshold` are sent to the qualification LLM, which writes an assessment and a draft for a human to review.

## Schema reference

All objects reject unknown fields. Every field below is required unless a default is given. Paths in error messages use this naming (`sources.http_probe.fingerprints[0].path`).

### Root

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `schema_version` | integer | must be `1` | Selects the definition version. Only 1 exists. |
| `identity` | object | | Free text, goes to the LLM. |
| `sources` | object | at least one source | What to scan. |
| `scoring` | object | | How leads are scored. |
| `contact_ranking` | object | | Which role addresses to keep and prefer. |
| `llm` | object | | Qualification threshold and relevance criteria. |

### `identity`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `offer` | string | not blank | What you sell. First line of the LLM market context. Also the default profile name when none is given at import (max 120 characters). |
| `target_customer` | string | not blank | Who buys it. Second line of the LLM market context. |

### `sources`

Each key is optional. A missing key disables the source for this profile. At least one must be present, else `sources: no source is enabled`.

| Key | Type | What it does |
|---|---|---|
| `certificate_transparency` | object | Finds candidate hosts from hostname labels in certificates. Produces no signal by itself. |
| `http_probe` | object | Fetches fingerprint paths on candidate hosts. Produces `product_detected` and `debt_marker` signals. |
| `dns_cname` | object | Resolves candidate hosts' CNAME chains. Produces `saas_provider` signals. |
| `boamp` | keyword source | Open French tenders. Produces `public_tender`. |
| `decp` | keyword source | Awarded French public contracts. Produces `awarded_contract`. |
| `ted` | keyword source | Open EU tenders. Produces `public_tender`. |
| `france_travail` | keyword source | Job offers from the France Travail API. Produces `job_offer`. Disabled at runtime unless the operator set credentials ([configuration](configuration.md)). |
| `ats` | keyword source | Open jobs on Greenhouse, Lever or Ashby boards of already detected leads. Produces `job_offer`. |

#### `certificate_transparency`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `labels` | array of string | not empty; each a lowercase hostname label: `a-z`, `0-9`, `-`, 1 to 63 characters, not starting or ending with `-` | A certificate name matches when a label of its subdomain part equals one of these. `sso.acme.com` matches `sso`; `acme.com` alone matches nothing; `*.` prefixes are stripped first. |

Labels of all active profiles are merged into one filter. Matching hosts are stored once, globally, as candidate hosts; the HTTP probe and DNS CNAME sources then examine every candidate host, whichever profile's labels found it (`libs/sources/src/ct.rs`, `libs/core/src/infrastructure/candidate_host/postgres/hosts.rs`). A wildcard certificate hides the labels below it (ADR [0003](adr/0003-read-ct-from-tiled-logs.md)). Without `http_probe` or `dns_cname`, CT labels produce no lead for this profile.

Candidate hosts can also be loaded from a file with the `import_hosts` binary of `apps/api` (one hostname per line on stdin).

#### `http_probe`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `fingerprints` | array of fingerprint | not empty; `id` unique | Checks run on each candidate host. |
| `excluded_products` | array of string, default `[]` | no blank entry | If any fingerprint matching a host has a `product` in this list, the host yields no signal at all. Use it to skip hosts that already run your own product. |

Fingerprint:

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `id` | string | not blank, unique in the probe | Name of the check, kept in the signal's raw data. |
| `path` | string | starts with `/`, contains no `://` | Fetched as `https://<host><path>`. Fingerprints with the same path share one request. |
| `product` | string | not blank | Value of the `product_detected` signal. |
| `matchers` | array of matcher | not empty | All must hold (logical AND). |
| `version` | version extractor, optional | regex with a capture group | Reads a version from the response. |
| `debt_markers` | array of debt marker, default `[]` | `id` unique in the fingerprint | Extra signals when the response shows outdated maintenance. |

Matcher, tagged by `kind`:

| `kind` | Fields | Holds when |
|---|---|---|
| `status` | `equals`: integer 100 to 599 | The HTTP status equals it. |
| `header` | `name`: not blank, `pattern`: regex | The named header (case-insensitive) exists and the pattern matches its value. |
| `body` | `pattern`: regex | The pattern matches the first 256 KiB of the body. |

Version extractor, tagged by `from`: `{"from": "header", "name": "...", "pattern": "..."}` or `{"from": "body", "pattern": "..."}`. The pattern needs at least one capture group; group 1 is the version.

Debt marker: `{"id": "...", "rule": ...}`, with `rule` tagged by `kind`:

| `kind` | Fields | Fires when |
|---|---|---|
| `matches` | `matcher`: a matcher | The matcher holds on the same response. |
| `version_below` | `version`: dotted numeric (`2.4`, `22.0.1`) | A version was extracted and is lower. Missing components count as 0. Needs a `version` extractor on the fingerprint, else `VersionBelowWithoutExtractor`. |

Regular expressions use the Rust `regex` syntax (no look-around, no back-references), search unanchored (add `^`/`$` to anchor), are at most 1024 characters and compile within a 1 MiB size limit. In JSON, escape backslashes (`\\s`).

Signals produced (`libs/sources/src/http_probe.rs`): `product_detected` with value `product`, strength 0.9, once per product per host; `debt_marker` with value the marker `id`, strength 0.6, once per marker id. Provenance is `https://<host><path>`. Only HTTPS is probed, and each host is probed at most once per `OUTBOUND_REVISIT_INTERVAL_HOURS`.

#### `dns_cname`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `patterns` | array of pattern | not empty | Provider checks. |

Pattern:

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `provider` | string | not blank | Value of the `saas_provider` signal. |
| `target_suffix` | string | lowercase domain suffix with at least one dot, labels as above | A hop of the CNAME chain matches when it equals the suffix or ends with `.<suffix>`. |

One signal per provider per host, strength 0.9, provenance `dns:<host>?type=CNAME` (`libs/sources/src/dns_cname.rs`).

#### Keyword sources: `boamp`, `decp`, `ted`, `france_travail`, `ats`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `keywords` | array of string | not empty, no blank entry | Terms to look for. |
| `exclusions` | array of string, default `[]` | no blank entry | Terms that stop an automatic accept. |
| `accept_min_keywords` | integer, default `2` | 1 to 255 | Distinct keywords needed for an accept without the LLM. |

Routing is described in [Keywords and exclusions](#keywords-and-exclusions).

### `scoring`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `factors` | array of factor | not empty | Terms of the score. |

Factor: `signal` (string, not blank, free text), `value` (string, optional, not blank), `weight` (finite number, negative allowed). See [Scoring](#scoring-1).

### `contact_ranking`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `preferred_local_parts` | array of string | no blank entry, no `@`, no whitespace, no duplicate | Local parts of addresses worth keeping, best first. Matching is case-insensitive. |
| `guess_local_parts` | array of string, default `[]` | each also in `preferred_local_parts`, no duplicate | Local parts that may be guessed (`<part>@<domain>`) when a domain has MX records and no public address was found. |

### `llm`

| Field | Type | Constraint | Effect |
|---|---|---|---|
| `qualification_threshold` | number | finite | Minimum score (inclusive) for a `new` lead to be qualified. |
| `relevance_criteria` | string | not blank | Third line of the LLM market context. |

## Writing fingerprints

Pick a public URL that answers differently when your target product is present. Good candidates are login or discovery endpoints, health endpoints and version endpoints. Rules:

- Combine a `status` matcher with a `body` or `header` matcher. A status alone matches too many hosts.
- Anchor patterns on something unique to the product (a JSON key, a title, a header value).
- Probes are plain GET requests with no credentials. A fingerprint needing a POST or a login cannot be expressed.
- Several products can share one path (the IAM example uses `/realms/master` for Keycloak and for FerrisKey, told apart by status and body).
- To read a version, add `version` and write one capture group. The Magento example reads `^Magento/(\d+\.\d+)` from the body and uses `version_below` `2.4`.
- Use `excluded_products` for products that make a host a bad lead.
- Test a pattern against a real response before importing. Regex errors are caught on import (`InvalidPattern`), but a pattern that is valid and never matches is not.

Each fixture-based test in `libs/sources/tests/` (`iam_fixtures.rs`, `http_probe_fixtures.rs`) shows how recorded responses are replayed against a profile.

## Keywords and exclusions

For `boamp`, `decp`, `ted`, `france_travail` and `ats`, Lodestone builds a text for each candidate (title plus description) and routes it with `route` in `libs/core/src/domain/triage/mod.rs`:

1. The text and each term are lowercased and whitespace is collapsed. Accents are not folded: write `identités` with the accent.
2. A term matches as a whole word or phrase: it must not touch a letter or digit. `paie` does not match `paiement`. `iam` does not match `iamistes`.
3. Duplicate keywords count once.
4. No keyword found: reject. The candidate is dropped.
5. At least `accept_min_keywords` distinct keywords found and no exclusion found: accept. A signal is stored with strength 0.7 and the matched keywords in its raw data.
6. Otherwise (too few keywords, or any exclusion found): ambiguous. The candidate goes to the triage LLM. If no triage provider is assigned, it is skipped for this run.

An exclusion never accepts by itself: a text with an exclusion and no keyword is rejected.

The triage LLM accepts only if it answers `relevant` and quotes an exact excerpt of the text. An accepted candidate becomes a signal with strength 0.6. The model may also return a role mailbox of the company; it is stored as a contact after the usual checks.

Keywords also drive the upstream query, except for `ats` and `decp`:

- `boamp`, `ted`, `france_travail`: the keywords of all active profiles are sent to the API as a search (`libs/sources/src/boamp.rs`, `ted.rs`, `france_travail.rs`). A candidate whose text lacks every keyword is never fetched.
- `decp`: the monthly files are downloaded whole and filtered locally with the same routing. Winning companies of matching contracts are also recorded in the tables `partners` and `partner_awards`. No API route reads them in this repository.
- `ats`: runs only for leads that already have a signal from certificate transparency, HTTP probe or DNS CNAME (`libs/core/src/infrastructure/contact_target/postgres/ats.rs`). The board is found from the lead's own pages, then all its jobs are read and routed. A profile with `ats` and none of those three sources gets no ATS signal.

How to choose values:

- Use phrases a buyer or recruiter writes, in the language of the source. BOAMP, DECP and France Travail text is French.
- Put generic words in `keywords` and rely on `accept_min_keywords: 2` so one generic hit goes to the LLM instead of straight to a lead.
- Use `accept_min_keywords: 1` only for specific terms, such as a job title.
- Use `exclusions` for neighbouring topics. The IAM example excludes `badge` and `contrôle d'accès physique`.

The signal `value` of a keyword source is the candidate title, cut to 200 characters. A scoring factor with a `value` on `public_tender` or `job_offer` is therefore rarely useful.

## Scoring

The score of a lead is the sum over `scoring.factors` of `weight × measure` (`libs/core/src/domain/scoring/mod.rs`):

- For a normal factor, `measure` is the highest strength (0 to 1) among the lead's signals whose kind equals `signal` and, if the factor has a `value`, whose value equals it. Several matching signals count once. No match gives 0.
- For the reserved `signal` `distinct_signals`, `measure` is the number of distinct signal kinds on the lead. `value` is ignored.
- The score is not clamped or normalized, and negative weights subtract.

The score and the contribution of each factor are stored on the lead and shown in the UI. They are recomputed when new signals or contacts are recorded for the lead.

Signal kinds the code emits today, with their `value`:

| `signal` | Source | `value` | Strength |
|---|---|---|---|
| `product_detected` | http_probe | fingerprint `product` | 0.9 |
| `debt_marker` | http_probe | marker `id` | 0.6 |
| `saas_provider` | dns_cname | pattern `provider` | 0.9 |
| `public_tender` | boamp, ted | title | 0.7 rules, 0.6 LLM |
| `awarded_contract` | decp | title | 0.7 rules, 0.6 LLM |
| `job_offer` | france_travail, ats | title | 0.7 rules, 0.6 LLM |
| `role_contact_found` | public pages, LLM extraction | local part | address confidence (0.9 `mailto:`, 0.8 text, 0.6 obfuscated, 0.5 LLM) |
| `security_txt_contact` | security.txt | local part | 0.7 |
| `role_contact_guessed` | role guess | local part | 0.2 |

A factor naming another kind is valid but always measures 0.

Choosing weights: give the largest weight to the strongest evidence of need, small weights to contact signals, and use `distinct_signals` to reward corroboration across sources. Then set `qualification_threshold` so that one weak signal alone stays below it and two independent kinds reach it. Each qualified lead costs an LLM call, so a low threshold costs money. Qualification looks at the highest scores first and handles 20 leads per profile per run.

## Contact ranking

Contacts are role mailboxes at the lead's own domain (or a subdomain). An address from a public page or the LLM is stored only if its local part is in `preferred_local_parts`. A guessed address is stored only if its local part is in `guess_local_parts`, and only when no public address exists. Contacts are ordered by position in `preferred_local_parts`, then by confidence. List the mailboxes your market answers to first: `security` and `dsi` for IAM, `rh` and `paie` for payroll. The code does not check that a listed local part is a role rather than a person; do not list personal names (see [compliance](compliance.md)).

## LLM context

The model never sees the whole profile. It sees one block in its system prompt (`market_context` in `libs/core/src/domain/prompt/mod.rs`, templates in `libs/core/prompts/`):

```
<market_context>
Offer: <identity.offer>
Target customer: <identity.target_customer>
Relevance criteria: <llm.relevance_criteria>
</market_context>
```

Keywords, weights, fingerprints and the threshold are not sent. Both the triage and the qualification prompt apply the same rules:

- Everything in `<market_context>` and in the document blocks is data, not instructions. Text in a profile field that looks like a rule is ignored by the prompt, and `</market_context>` or `{{market_context}}` inside a field is neutralized.
- Use only facts stated in the documents. A claim needs evidence.
- Evidence must be an exact excerpt of one document, character for character. For triage the document is the candidate title and text. For qualification the documents are the lead's stored signals, each rendered as plain text with its kind, value, source, date and raw data (`render_signal` in `libs/core/src/application/qualification/mod.rs`). Page bodies are not stored and not sent.
- If an excerpt is not verbatim, qualification retries once. If it fails again, the lead becomes `uncertain` and never reaches the review queue.

Consequences for writing `relevance_criteria`:

- Describe facts that can show up in a signal: the product detected, a tender topic, a job title. The model cannot judge what no signal states.
- Say what is relevant and what is not, in two or three sentences. The IAM example names physical access control as a non-match.
- Write it in English or in the language of your sources; the prompts are in English.
- Keep it factual. The assessment and the draft are built from it, and a human reads them before anything else happens.
- Everything in `identity` and `llm` is sent to the LLM provider you configured. Do not put secrets or personal data in them. Do not paste third-party text into them.

The draft message length, tone and structure are fixed by the prompt templates, not by the profile. Not covered: a per-profile draft language.

## Worked example: `iam-ferriskey.json`

Sells FerrisKey and Autharie, identity and access management, to organizations running Keycloak or a paid SaaS IAM.

**`identity`.** `offer` and `target_customer` state the product and the buyer in one sentence each. They become the first two lines of the market context.

**`sources.certificate_transparency.labels`.** Nine labels: `auth`, `sso`, `login`, `iam`, `keycloak`, `id`, `accounts`, `identity`, `idp`. Any certificate for a name like `sso.example.com` or `login.eu.example.com` makes that host a candidate. The result feeds the probe and the DNS check below.

**`sources.http_probe`.** Five fingerprints:

- `keycloak-realm`: `GET /realms/master`, status 200 and a body matching `"realm"\s*:\s*"master"`. Product `keycloak`.
- `keycloak-legacy-realm`: `/auth/realms/master`, same matchers, with a debt marker `legacy_auth_path`: it fires when the body matches `/auth/realms/master`. The old `/auth` prefix points to a Keycloak not migrated to recent releases.
- `authentik`: `/-/health/live/`, status 200 and header `x-powered-by` equal to `authentik`.
- `zitadel`: `/ui/console`, status 200 and body containing `<title>ZITADEL`.
- `ferriskey`: `/realms/master`, status 401 and body code `E_UNAUTHORIZED`. Product `ferriskey`.

`excluded_products: ["ferriskey"]` drops any host where the last fingerprint matches: someone already running FerrisKey is not a lead. The first and last fingerprints share the path `/realms/master`, which is requested once per host.

**`sources.dns_cname`.** Five patterns map CNAME targets to providers: `auth0.com` to `auth0`, `okta.com` and `oktapreview.com` to `okta`, `b2clogin.com` to `azure_ad_b2c`, `ciamlogin.com` to `entra_external_id`. A host `login.acme.com` that is a CNAME to `acme.okta.com` yields a `saas_provider` signal with value `okta`.

**`boamp`, `decp`, `ted`.** The same eight keywords (`SSO`, `IAM`, `gestion des identités et des accès`, `fédération d'identité`, `annuaire`, `MFA`, `Keycloak`, `authentification`) and three exclusions (`badge`, `contrôle d'accès physique`, `annuaire téléphonique`). `accept_min_keywords` is the default 2. A notice that says "SSO" and "MFA" is accepted. A notice that says only "annuaire" goes to the LLM. A notice that says "SSO", "MFA" and "badge" goes to the LLM because of the exclusion.

**`france_travail`, `ats`.** Six keywords such as `Keycloak`, `ingénieur IAM`, `migration Okta`. No exclusions. The ATS source reads jobs only for leads found by CT, probe or DNS.

**`scoring`.** Seven factors:

| Factor | Weight | Meaning |
|---|---|---|
| `product_detected` = `keycloak` | 30 | Keycloak found. |
| `debt_marker` = `legacy_auth_path` | 15 | Legacy `/auth` path. |
| `saas_provider` | 10 | Any detected SaaS IAM provider. |
| `job_offer` | 25 | Hiring around the keywords. |
| `public_tender` | 20 | Tender around the keywords. |
| `role_contact_found` | 5 | A role address was found. |
| `distinct_signals` | 5 | Per distinct signal kind. |

Example: a host with Keycloak found (strength 0.9), a job offer accepted by rules (0.7) and a contact found on a public page (0.8). Contributions: 30 × 0.9 = 27, 25 × 0.7 = 17.5, 5 × 0.8 = 4, and `distinct_signals` 5 × 3 kinds = 15. Total 63.5. Without the job offer it is 27 + 4 + 10 = 41, below the threshold of 50, so it is not qualified yet.

**`contact_ranking`.** `security`, `it`, `dsi`, `tech`, `contact`, `info` in this order. Only `contact` and `info` may be guessed.

**`llm`.** Threshold 50. The criteria say the organization runs its own identity infrastructure or pays for a SaaS IAM and shows migration, staffing or procurement signs, and that physical access control and unrelated directories are not relevant.

## Second example: `payroll-saas.json`

A different market, built from keyword sources only: [`examples/profiles/payroll-saas.json`](examples/profiles/payroll-saas.json). It sells payroll software to French employers.

- No CT, probe or DNS source: nothing here can be fingerprinted from outside, so the signals are tenders and job offers.
- `boamp` uses default `accept_min_keywords` (2). A text with `paie` alone goes to the LLM. Overlapping keywords count separately, so a text containing `logiciel de paie` matches both `logiciel de paie` and `paie` and is accepted. Avoid overlaps if you want the threshold to mean what it says. Exclusions mark outsourcing tenders, which are not a fit for software.
- `france_travail` sets `accept_min_keywords: 1`: a job title such as `gestionnaire de paie` is specific enough to accept alone. `cabinet comptable` is an exclusion, so such offers go to the LLM.
- `ats` is left out on purpose: it runs only for leads found by CT, probe or DNS, and this profile has none of them.
- Scoring: `job_offer` 30, `public_tender` 25, contacts 5, `distinct_signals` 10, threshold 40. One job offer accepted by rules scores 30 × 0.7 + 10 = 31, below 40. A job offer and a tender score 21 + 17.5 + 20 = 58.5, above it.
- Contacts: `rh`, `paie`, `drh`, `contact`, `info`; `rh` and `contact` may be guessed.

The file is checked by `ValidatedProfile::parse` like every example. See "Not covered" at the end about the test list.

## Import and activate

A new profile is created inactive with revision 1. Only active profiles are scheduled.

### Web UI

1. Sign in. Open **Profiles** in the Configuration menu.
2. In **Import a profile**, choose the JSON file. Optionally type a name; the default is `identity.offer`.
3. On success the profile appears in the list. If the file is invalid, **The file was rejected** shows the path and the problem.
4. Press **Activate** in the profile's row of the list, or on the profile page.

**New profile** creates a profile from a name and a definition typed in the editor. Editing a profile saves a new revision.

### API

The base path is `/api/v1`. Write routes need a bearer token of a user; a service account token gets 403. The public server listens on `SERVER_PORT` (default 3456). How to obtain a token depends on your FerrisKey setup and is not described here.

```bash
export API=http://localhost:3456/api/v1
export TOKEN=<user access token>

curl -sS -X POST "$API/profiles/import?name=payroll-saas" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  --data-binary @docs/examples/profiles/payroll-saas.json
```

`201` returns `{"data": {"id": "...", "name": "payroll-saas", "active": false, "revision": 1, "definition": {...}}}` and a `Location` header. The body of `import` is the bare definition. `POST /profiles` takes `{"name": "...", "definition": {...}}` instead. `409` means the name is taken.

```bash
curl -sS -X POST "$API/profiles/<id>/activate" -H "Authorization: Bearer $TOKEN"
```

Other routes: `GET /profiles`, `GET /profiles/{id}` (current revision), `GET /profiles/{id}/revisions`, `GET /profiles/{id}/revisions/{n}`, `GET /profiles/{id}/export` (raw definition, ready to import elsewhere), `POST /profiles/{id}/deactivate` (leads are kept). The OpenAPI document is served at `/api-docs/openapi.json` and `/scalar` only when the operator sets `SERVER_ENABLE_API_DOCS=true`.

## Revisions

`PUT /api/v1/profiles/{id}` with `{"definition": {...}}` validates the document and compares it with the current revision (`libs/core/src/domain/profile/service.rs`):

- Identical definition: `200`, the current revision is returned, nothing is created.
- Different definition: `201`, a new revision `n+1` is appended. Older revisions are immutable.

What a new revision changes:

- Workers use the latest revision of an active profile on their next run. Compiled probe rules are keyed by profile and revision.
- LLM cache: entries are keyed by profile, revision, prompt version, domain and the full prompt. Any new revision misses the cache, so triage and qualification call the model again for the same inputs. This costs money; see `LLM_RUN_BUDGET_CENTS`.
- Qualification: a `new` lead is qualified if it has no assessment for the current revision and prompt version. Leads already `qualified`, `validated`, `rejected` or `uncertain` are not queued again by the new revision. To qualify one again, move it back to `new` through `PATCH /api/v1/leads/{id}/status` (allowed from `qualified`, `rejected` and `uncertain`).
- Scores: not recomputed when a revision is created. A lead is rescored, with the latest revision's `scoring`, the next time a signal or contact is recorded for it. Until then it keeps its old score and breakdown.
- Existing signals are kept. Signals gathered under old fingerprints or keywords are not deleted.

## Validation errors

Validation runs in two passes (ADR 0002): the JSON is parsed into the typed definition, which yields a path for structural errors, then semantic checks run. Only the first problem is reported, so fix and resubmit.

The API answers `422`:

```json
{"code": "E_UNPROCESSABLE_ENTITY", "message": "Request payload rejected", "status": 422,
 "details": [{"path": "sources.http_probe.fingerprints[0].matchers[1].pattern", "message": "invalid regular expression"}]}
```

`path` is a dotted path with array indexes; `$` is the document root. Messages seen in practice, produced by running the validator on modified copies of the examples:

| Path | Message | Cause |
|---|---|---|
| `scoring.factors[0].weight` | `malformed: invalid type: string "high", expected f64` | Wrong JSON type. |
| `sources.boamp.keyword` | `malformed: unknown field \`keyword\`, expected one of \`keywords\`, \`exclusions\`, \`accept_min_keywords\`` | Typo. Unknown fields are rejected. |
| `sources.ats.accept_min_keywords` | `must be at least 1` | `0` given. |
| `contact_ranking.guess_local_parts[0]` | `must also be listed in \`preferred_local_parts\`` | Guess not ranked. |
| `sources.http_probe.fingerprints[0].matchers[1].pattern` | `invalid regular expression` | Does not compile, too long or too big. |
| `sources.certificate_transparency.labels[0]` | `must be a lowercase hostname label` | `Auth` instead of `auth`. |
| `sources.dns_cname.patterns[0].target_suffix` | `must be a lowercase domain suffix with at least one dot` | `com` instead of `okta.com`. |
| `sources.http_probe.fingerprints[1].id` | ``duplicate value `keycloak-realm` `` | Second fingerprint reuses an id. |
| `sources.http_probe.fingerprints[1].debt_markers[0].rule` | `` `version_below` needs a version extractor on the fingerprint `` | Add `version`. |
| `sources.http_probe.fingerprints[0].path` | `must be an absolute path starting with \`/\`` | `realms/master`. |
| `sources` | `no source is enabled` | Empty `sources`. |
| `schema_version` | `unsupported schema version 2, expected 1` | Wrong version. |

A missing required field is reported at the path of its parent, or at `$` for a missing top-level field. `404`, `409` and `403` responses carry no `details`.

## Not covered

- How to obtain a user access token for `curl`: it depends on the FerrisKey realm configuration.
- A per-profile language or tone for drafts: the prompts in `libs/core/prompts/` are not configurable from a profile.
- `libs/core/tests/profile_examples.rs` lists examples with `include_str!` and does not scan the directory. `payroll-saas.json` was validated with `ValidatedProfile::parse` outside that test, and needs a line added to the test to be checked by CI.
