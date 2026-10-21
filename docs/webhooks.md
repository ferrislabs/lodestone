# Webhooks and exports

A lead that becomes `validated` (draft approved, or status moved to `validated`) is posted to `WEBHOOK_URL`. The delivery is a `webhook.deliver` job enqueued in the same transaction as the status change, so it is at-least-once: deduplicate on `Lodestone-Event-Id`. The suppression list is checked again when the job runs; a suppressed lead is never sent.

Configuration: `WEBHOOK_URL`, `WEBHOOK_SECRET` (32 bytes minimum, from a Kubernetes secret), `WEBHOOK_PROFILE_IDS` (optional filter), `WEBHOOK_TIMEOUT_SECONDS` (default 10). See `.env.example`.

The target is operator-configured, so the public-target refusal of the polite fetcher does not apply: internal hosts are allowed. Redirects are not followed and TLS is verified.

A 2xx answer completes the delivery. Any other status or a network error is retried with the job queue backoff, up to `JOBS_MAX_ATTEMPTS`. `410 Gone` ends the delivery without retry.

## Request

```
POST <WEBHOOK_URL>
Content-Type: application/json
Lodestone-Event-Id: 5b7c0a3e-9a52-8d0b-b3c4-1f2e3d4c5b6a
Lodestone-Signature: t=1800000000,v1=<hex hmac-sha256(secret, "<t>.<raw body>")>
```

```json
{
  "id": "5b7c0a3e-9a52-8d0b-b3c4-1f2e3d4c5b6a",
  "type": "lead.validated",
  "occurred_at": "2027-01-15T08:00:00Z",
  "profile_id": "0197c3a0-0000-7000-8000-000000000002",
  "data": {
    "lead_id": "0197c3a0-0000-7000-8000-000000000001",
    "profile": {"id": "0197c3a0-0000-7000-8000-000000000002", "name": "keycloak-ops", "revision": 3},
    "domain": "acme.com",
    "status": "validated",
    "score": 42.0,
    "first_seen_at": "2027-01-10T08:00:00Z",
    "last_seen_at": "2027-01-15T07:00:00Z",
    "contact": {"address": "security@acme.com", "method": "public_page", "confidence": 0.8, "provenance": "https://acme.com/contact"},
    "assessment": {
      "summary": "Runs Keycloak",
      "approach_angle": "Managed IAM",
      "evidence": [{"quote": "Powered by Keycloak", "provenance": "https://acme.com/about"}],
      "draft": {"subject": "Hello", "body": "...", "status": "approved"}
    },
    "signals": [{"kind": "product_detected", "value": "keycloak", "source": "http_probe", "provenance": "https://acme.com/", "observed_at": "2027-01-15T07:00:00Z"}]
  }
}
```

`contact` and `assessment` are `null` when absent or suppressed.

## Verifying the signature

Sign the raw body bytes, not a re-serialised JSON. Reject timestamps more than 5 minutes from your clock, and compare in constant time.

```python
import hmac, hashlib, time

def verify(secret: bytes, header: str, raw_body: bytes, tolerance: int = 300) -> bool:
    parts = dict(p.split("=", 1) for p in header.split(","))
    t = int(parts["t"])
    if abs(time.time() - t) > tolerance:
        return False
    expected = hmac.new(secret, f"{t}.".encode() + raw_body, hashlib.sha256).hexdigest()
    return hmac.compare_digest(expected, parts["v1"])
```

A Rust reference is `api::webhook::verify`.

## Exports

`GET /api/v1/profiles/{profile_id}/leads/export?format=json|csv` returns the profile leads as an attachment named `<profile>-leads.<ext>`, with the same filters as the lead list (`status`, `min_score`, `domain`, `signal`, `sort`). Suppressed domains and addresses are excluded. An export holds at most 10000 leads, beyond that the request is refused with 422 and the filters must be narrowed. Service accounts may call it. CSV cells starting with `=`, `+`, `-`, `@`, tab or CR are prefixed with `'`.

In the payload and in the exports, `score` is the signal strength of the lead from 0 to 100: the strongest independent signal raised by the next two, each strength given by its source and decayed by its age, so a weaker signal never lowers it (`min_score` and the `score_desc` and `score_asc` sorts use it too). It was a weighted sum in points before profile schema 13. The decision of the scoring agents is not exported in this field.
