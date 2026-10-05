# Metrics

Lodestone exposes Prometheus metrics on the internal server (`SERVER_INTERNAL_PORT`), path `/metrics`, never on the public server. The body is OpenMetrics text (`Content-Type: application/openmetrics-text; version=1.0.0; charset=utf-8`), which Prometheus scrapes natively. All families are described (`# HELP`, `# TYPE`) even before the first event.

The registry is built with the `prometheus-client` crate (`libs/telemetry`) and shared by the job workers, the use cases and the internal router. Counters reset when the process restarts, use `rate()` / `increase()`.

## Label values

Every label is bounded. Domains, URLs and lead ids are never labels.

| Label | Values |
| --- | --- |
| `source` | a source name (`certificate_transparency`, `dns_cname`, `http_probe`, `public_page`, `security_txt`, `role_guess`, `boamp`, `decp`, `ted`, `france_travail`, `ats`) or `llm_qualification` |
| `profile` | the profile id (UUID), bounded by the number of profiles |
| `outcome` (runs) | `succeeded`, `failed` |
| `outcome` (jobs) | `done`, `rescheduled`, `retry`, `failed`, `lease_lost`, `released` |
| `outcome` (LLM) | `succeeded`, `invalid_output`, `schema_error`, `timeout`, `rate_limited`, `provider_error` |
| `kind` (signals) | the signal kinds defined by profiles (`product_detected`, `public_tender`, ...) |
| `kind` (errors) | `upstream`, `rate_limited`, `budget`, `other` |
| `kind` (jobs) | the job kind (`source.run`, `llm.qualify`, `webhook.deliver`) |
| `status` | `pending`, `running`, `failed` |
| `purpose` | the LLM tier: `triage`, `qualification` |
| `provider_kind` | `anthropic`, `mistral`, `openai_compatible` (`unknown` when no provider is bound) |
| `model` | the configured model of the provider in use, bounded by the configured providers |
| `cached` | `true`, `false` |
| `direction` | `input`, `output` |

## Families

| Name | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `lodestone_source_runs_total` | counter | `source`, `profile`, `outcome` | Finished runs, one per profile |
| `lodestone_source_run_duration_seconds` | histogram | `source` | Run duration |
| `lodestone_source_items_total` | counter | `source`, `profile` | Items collected by runs |
| `lodestone_signals_recorded_total` | counter | `source`, `profile`, `kind` | Signals actually stored (suppressed domains excluded). Contacts found by contact finders are not signals |
| `lodestone_source_errors_total` | counter | `source`, `kind` | Failed runs by cause. `budget` counts runs stopped by the LLM budget |
| `lodestone_jobs_total` | counter | `kind`, `outcome` | Jobs processed by workers |
| `lodestone_job_duration_seconds` | histogram | `kind` | Time a worker spent on a job |
| `lodestone_job_wait_seconds` | histogram | `kind` | Time between `run_at` and the claim |
| `lodestone_jobs_queue` | gauge | `kind`, `status` | Jobs in the table, read at scrape time (2 s timeout; the previous values are dropped when the read fails) |
| `lodestone_llm_calls_total` | counter | `purpose`, `provider_kind`, `model`, `profile`, `outcome`, `cached` | Calls to the LLM gateway, one per call including its retries. Cache hits have `cached="true"` |
| `lodestone_llm_tokens_total` | counter | `purpose`, `provider_kind`, `model`, `profile`, `direction` | Tokens of the successful attempt |
| `lodestone_llm_cost_microcents_total` | counter | `purpose`, `provider_kind`, `model`, `profile` | Spend in microcents (1 cent = 1,000,000), including failed retries, priced with the provider's per-Mtok prices |
| `lodestone_llm_budget_exhausted_total` | counter | `purpose`, `profile` | Calls refused because the run budget was spent |

Duration histograms use exponential buckets from 0.05 s to about 985 s (factor 3).

## PromQL

LLM cost per profile per day, in cents:

```promql
sum by (profile) (increase(lodestone_llm_cost_microcents_total[1d])) / 1e6
```

Cost per profile and model:

```promql
sum by (profile, model) (increase(lodestone_llm_cost_microcents_total[1d])) / 1e6
```

Cache hit ratio of the LLM calls:

```promql
sum(rate(lodestone_llm_calls_total{cached="true"}[1h])) / sum(rate(lodestone_llm_calls_total[1h]))
```

Failed runs per source over the last day:

```promql
sum by (source) (increase(lodestone_source_runs_total{outcome="failed"}[1d]))
```

Signals per profile per day:

```promql
sum by (profile) (increase(lodestone_signals_recorded_total[1d]))
```

95th percentile of the job wait, per kind:

```promql
histogram_quantile(0.95, sum by (le, kind) (rate(lodestone_job_wait_seconds_bucket[15m])))
```

Backlog of due jobs:

```promql
sum by (kind) (lodestone_jobs_queue{status="pending"})
```
