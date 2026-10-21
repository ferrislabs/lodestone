use std::{
    fmt,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{
        counter::Counter,
        family::Family,
        gauge::Gauge,
        histogram::{Histogram, exponential_buckets},
    },
    registry::{Registry, Unit},
};

pub const CONTENT_TYPE: &str = "application/openmetrics-text; version=1.0.0; charset=utf-8";

const BUCKET_START_SECONDS: f64 = 0.05;
const BUCKET_FACTOR: f64 = 3.0;
const BUCKET_COUNT: u16 = 10;
const QUEUE_STATUSES: [&str; 3] = ["pending", "running", "failed"];

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SourceProfile {
    source: String,
    profile: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SourceRun {
    source: String,
    profile: String,
    outcome: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SourceOnly {
    source: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SignalLabels {
    source: String,
    profile: String,
    kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct SourceError {
    source: String,
    kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct JobOutcome {
    kind: String,
    outcome: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct JobKind {
    kind: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct QueueLabels {
    kind: String,
    status: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct LlmCallLabels {
    purpose: String,
    provider_kind: String,
    model: String,
    profile: String,
    outcome: String,
    cached: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct LlmTokenLabels {
    purpose: String,
    provider_kind: String,
    model: String,
    profile: String,
    direction: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct LlmCostLabels {
    purpose: String,
    provider_kind: String,
    model: String,
    profile: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct BudgetLabels {
    purpose: String,
    profile: String,
}

type HistogramFamily<L> = Family<L, Histogram, fn() -> Histogram>;

fn histogram() -> Histogram {
    Histogram::new(exponential_buckets(
        BUCKET_START_SECONDS,
        BUCKET_FACTOR,
        BUCKET_COUNT,
    ))
}

fn histograms<L>() -> HistogramFamily<L>
where
    L: Clone + std::hash::Hash + Eq,
{
    Family::new_with_constructor(histogram as fn() -> Histogram)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmIdentity<'a> {
    pub purpose: &'a str,
    pub provider_kind: &'a str,
    pub model: &'a str,
    pub profile: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmCall<'a> {
    pub identity: LlmIdentity<'a>,
    pub outcome: &'a str,
    pub cached: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_microcents: u64,
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Counter,
    Gauge,
    Histogram,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Counter => "counter",
            Self::Gauge => "gauge",
            Self::Histogram => "histogram",
        }
    }
}

struct Descriptor {
    name: String,
    kind: Kind,
    help: &'static str,
    seconds: bool,
}

impl Descriptor {
    fn write_missing(&self, body: &mut String) {
        if body.contains(&format!("# TYPE {} ", self.name)) {
            return;
        }
        body.push_str(&format!("# TYPE {} {}\n", self.name, self.kind.as_str()));
        if self.seconds {
            body.push_str(&format!("# UNIT {} seconds\n", self.name));
        }
        body.push_str(&format!("# HELP {} {}\n", self.name, self.help));
    }
}

struct Catalog {
    registry: Registry,
    descriptors: Vec<Descriptor>,
}

impl Catalog {
    fn add<M>(&mut self, name: &str, help: &'static str, kind: Kind, seconds: bool, metric: M)
    where
        M: prometheus_client::registry::Metric,
    {
        if seconds {
            let base = name.trim_end_matches("_seconds");
            self.registry
                .register_with_unit(base, help, Unit::Seconds, metric);
        } else {
            self.registry.register(name, help, metric);
        }
        self.descriptors.push(Descriptor {
            name: name.to_owned(),
            kind,
            help,
            seconds,
        });
    }
}

struct Inner {
    registry: Registry,
    descriptors: Vec<Descriptor>,
    scrape: Mutex<()>,
    source_runs: Family<SourceRun, Counter>,
    source_run_duration: HistogramFamily<SourceOnly>,
    source_items: Family<SourceProfile, Counter>,
    signals_recorded: Family<SignalLabels, Counter>,
    source_errors: Family<SourceError, Counter>,
    jobs: Family<JobOutcome, Counter>,
    job_duration: HistogramFamily<JobKind>,
    job_wait: HistogramFamily<JobKind>,
    jobs_queue: Family<QueueLabels, Gauge>,
    llm_calls: Family<LlmCallLabels, Counter>,
    llm_tokens: Family<LlmTokenLabels, Counter>,
    llm_cost: Family<LlmCostLabels, Counter>,
    llm_budget_exhausted: Family<BudgetLabels, Counter>,
}

#[derive(Clone)]
pub struct Metrics {
    inner: Arc<Inner>,
}

impl fmt::Debug for Metrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Metrics").finish_non_exhaustive()
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        let mut catalog = Catalog {
            registry: Registry::default(),
            descriptors: Vec::new(),
        };
        let source_runs = Family::default();
        catalog.add(
            "lodestone_source_runs",
            "Source runs finished, per source, profile and outcome",
            Kind::Counter,
            false,
            source_runs.clone(),
        );
        let source_run_duration = histograms();
        catalog.add(
            "lodestone_source_run_duration_seconds",
            "Duration of a source run per source",
            Kind::Histogram,
            true,
            source_run_duration.clone(),
        );
        let source_items = Family::default();
        catalog.add(
            "lodestone_source_items",
            "Items collected by source runs, per source and profile",
            Kind::Counter,
            false,
            source_items.clone(),
        );
        let signals_recorded = Family::default();
        catalog.add(
            "lodestone_signals_recorded",
            "Signals recorded, per source, profile and signal kind",
            Kind::Counter,
            false,
            signals_recorded.clone(),
        );
        let source_errors = Family::default();
        catalog.add(
            "lodestone_source_errors",
            "Source failures, per source and kind (upstream, rate_limited, budget, other)",
            Kind::Counter,
            false,
            source_errors.clone(),
        );
        let jobs = Family::default();
        catalog.add(
            "lodestone_jobs",
            "Jobs processed by workers, per kind and outcome",
            Kind::Counter,
            false,
            jobs.clone(),
        );
        let job_duration = histograms();
        catalog.add(
            "lodestone_job_duration_seconds",
            "Time a worker spent running a job, per kind",
            Kind::Histogram,
            true,
            job_duration.clone(),
        );
        let job_wait = histograms();
        catalog.add(
            "lodestone_job_wait_seconds",
            "Time between a job becoming due and a worker claiming it, per kind",
            Kind::Histogram,
            true,
            job_wait.clone(),
        );
        let jobs_queue = Family::default();
        catalog.add(
            "lodestone_jobs_queue",
            "Jobs in the queue at scrape time, per kind and status (pending, running, failed)",
            Kind::Gauge,
            false,
            jobs_queue.clone(),
        );
        let llm_calls = Family::default();
        catalog.add(
            "lodestone_llm_calls",
            "LLM calls, per purpose, provider kind, model, profile, outcome and cache hit",
            Kind::Counter,
            false,
            llm_calls.clone(),
        );
        let llm_tokens = Family::default();
        catalog.add(
            "lodestone_llm_tokens",
            "LLM tokens of successful calls, per purpose, provider kind, model, profile and direction",
            Kind::Counter,
            false,
            llm_tokens.clone(),
        );
        let llm_cost = Family::default();
        catalog.add(
            "lodestone_llm_cost_microcents",
            "LLM spend in microcents (1e-6 cent), per purpose, provider kind, model and profile",
            Kind::Counter,
            false,
            llm_cost.clone(),
        );
        let llm_budget_exhausted = Family::default();
        catalog.add(
            "lodestone_llm_budget_exhausted",
            "LLM calls refused because the run budget was exhausted, per purpose and profile",
            Kind::Counter,
            false,
            llm_budget_exhausted.clone(),
        );
        let Catalog {
            registry,
            descriptors,
        } = catalog;
        Self {
            inner: Arc::new(Inner {
                registry,
                descriptors,
                scrape: Mutex::new(()),
                source_runs,
                source_run_duration,
                source_items,
                signals_recorded,
                source_errors,
                jobs,
                job_duration,
                job_wait,
                jobs_queue,
                llm_calls,
                llm_tokens,
                llm_cost,
                llm_budget_exhausted,
            }),
        }
    }

    pub fn source_run_finished(
        &self,
        source: &str,
        profile: &str,
        outcome: &str,
        items: u64,
        duration: Duration,
    ) {
        let inner = &self.inner;
        inner
            .source_runs
            .get_or_create(&SourceRun {
                source: source.to_owned(),
                profile: profile.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
        inner
            .source_run_duration
            .get_or_create(&SourceOnly {
                source: source.to_owned(),
            })
            .observe(duration.as_secs_f64());
        inner
            .source_items
            .get_or_create(&SourceProfile {
                source: source.to_owned(),
                profile: profile.to_owned(),
            })
            .inc_by(items);
    }

    pub fn signals_recorded(&self, source: &str, profile: &str, kind: &str, count: u64) {
        self.inner
            .signals_recorded
            .get_or_create(&SignalLabels {
                source: source.to_owned(),
                profile: profile.to_owned(),
                kind: kind.to_owned(),
            })
            .inc_by(count);
    }

    pub fn source_error(&self, source: &str, kind: &str) {
        self.inner
            .source_errors
            .get_or_create(&SourceError {
                source: source.to_owned(),
                kind: kind.to_owned(),
            })
            .inc();
    }

    pub fn job_claimed(&self, kind: &str, wait: Duration) {
        self.inner
            .job_wait
            .get_or_create(&JobKind {
                kind: kind.to_owned(),
            })
            .observe(wait.as_secs_f64());
    }

    pub fn job_finished(&self, kind: &str, outcome: &str, duration: Duration) {
        self.inner
            .jobs
            .get_or_create(&JobOutcome {
                kind: kind.to_owned(),
                outcome: outcome.to_owned(),
            })
            .inc();
        self.inner
            .job_duration
            .get_or_create(&JobKind {
                kind: kind.to_owned(),
            })
            .observe(duration.as_secs_f64());
    }

    pub fn llm_call(&self, call: &LlmCall<'_>) {
        let identity = call.identity;
        let inner = &self.inner;
        inner
            .llm_calls
            .get_or_create(&LlmCallLabels {
                purpose: identity.purpose.to_owned(),
                provider_kind: identity.provider_kind.to_owned(),
                model: identity.model.to_owned(),
                profile: identity.profile.to_owned(),
                outcome: call.outcome.to_owned(),
                cached: call.cached.to_string(),
            })
            .inc();
        for (direction, tokens) in [("input", call.input_tokens), ("output", call.output_tokens)] {
            inner
                .llm_tokens
                .get_or_create(&LlmTokenLabels {
                    purpose: identity.purpose.to_owned(),
                    provider_kind: identity.provider_kind.to_owned(),
                    model: identity.model.to_owned(),
                    profile: identity.profile.to_owned(),
                    direction: direction.to_owned(),
                })
                .inc_by(tokens);
        }
        inner
            .llm_cost
            .get_or_create(&LlmCostLabels {
                purpose: identity.purpose.to_owned(),
                provider_kind: identity.provider_kind.to_owned(),
                model: identity.model.to_owned(),
                profile: identity.profile.to_owned(),
            })
            .inc_by(call.cost_microcents);
    }

    pub fn llm_budget_exhausted(&self, purpose: &str, profile: &str) {
        self.inner
            .llm_budget_exhausted
            .get_or_create(&BudgetLabels {
                purpose: purpose.to_owned(),
                profile: profile.to_owned(),
            })
            .inc();
    }

    pub fn set_queue_depth(&self, rows: &[(String, String, i64)]) {
        let _scrape = self
            .inner
            .scrape
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.inner.jobs_queue.clear();
        for (kind, _, _) in rows {
            for status in QUEUE_STATUSES {
                self.inner
                    .jobs_queue
                    .get_or_create(&QueueLabels {
                        kind: kind.clone(),
                        status: status.to_owned(),
                    })
                    .set(0);
            }
        }
        for (kind, status, count) in rows {
            if QUEUE_STATUSES.contains(&status.as_str()) {
                self.inner
                    .jobs_queue
                    .get_or_create(&QueueLabels {
                        kind: kind.clone(),
                        status: status.clone(),
                    })
                    .set(*count);
            }
        }
    }

    pub fn render(&self) -> Result<String, fmt::Error> {
        let _scrape = self
            .inner
            .scrape
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut body = String::new();
        encode(&mut body, &self.inner.registry)?;
        let trimmed = body.trim_end().strip_suffix("# EOF").unwrap_or(&body).len();
        body.truncate(trimmed);
        for descriptor in &self.inner.descriptors {
            descriptor.write_missing(&mut body);
        }
        body.push_str("# EOF\n");
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILIES: [(&str, &str); 13] = [
        ("lodestone_source_runs", "counter"),
        ("lodestone_source_run_duration_seconds", "histogram"),
        ("lodestone_source_items", "counter"),
        ("lodestone_signals_recorded", "counter"),
        ("lodestone_source_errors", "counter"),
        ("lodestone_jobs", "counter"),
        ("lodestone_job_duration_seconds", "histogram"),
        ("lodestone_job_wait_seconds", "histogram"),
        ("lodestone_jobs_queue", "gauge"),
        ("lodestone_llm_calls", "counter"),
        ("lodestone_llm_tokens", "counter"),
        ("lodestone_llm_cost_microcents", "counter"),
        ("lodestone_llm_budget_exhausted", "counter"),
    ];

    #[test]
    fn every_family_is_described_before_any_event() {
        let body = Metrics::new().render().unwrap();
        for (name, kind) in FAMILIES {
            assert!(
                body.contains(&format!("# TYPE {name} {kind}")),
                "{name}\n{body}"
            );
            assert!(body.contains(&format!("# HELP {name} ")), "{name}\n{body}");
        }
        assert!(body.trim_end().ends_with("# EOF"));
    }

    #[test]
    fn events_carry_their_labels() {
        let metrics = Metrics::new();
        metrics.source_run_finished("boamp", "p1", "succeeded", 7, Duration::from_secs(2));
        metrics.signals_recorded("boamp", "p1", "public_tender", 3);
        metrics.source_error("boamp", "upstream");
        metrics.job_claimed("source.run", Duration::from_secs(1));
        metrics.job_finished("source.run", "rescheduled", Duration::from_secs(1));
        metrics.llm_budget_exhausted("triage", "p1");
        metrics.llm_call(&LlmCall {
            identity: LlmIdentity {
                purpose: "triage",
                provider_kind: "anthropic",
                model: "m",
                profile: "p1",
            },
            outcome: "succeeded",
            cached: false,
            input_tokens: 10,
            output_tokens: 2,
            cost_microcents: 42,
        });
        let body = metrics.render().unwrap();
        for line in [
            r#"lodestone_source_runs_total{source="boamp",profile="p1",outcome="succeeded"} 1"#,
            r#"lodestone_source_items_total{source="boamp",profile="p1"} 7"#,
            r#"lodestone_signals_recorded_total{source="boamp",profile="p1",kind="public_tender"} 3"#,
            r#"lodestone_source_errors_total{source="boamp",kind="upstream"} 1"#,
            r#"lodestone_jobs_total{kind="source.run",outcome="rescheduled"} 1"#,
            r#"lodestone_llm_budget_exhausted_total{purpose="triage",profile="p1"} 1"#,
            r#"lodestone_llm_calls_total{purpose="triage",provider_kind="anthropic",model="m",profile="p1",outcome="succeeded",cached="false"} 1"#,
            r#"lodestone_llm_tokens_total{purpose="triage",provider_kind="anthropic",model="m",profile="p1",direction="input"} 10"#,
            r#"lodestone_llm_tokens_total{purpose="triage",provider_kind="anthropic",model="m",profile="p1",direction="output"} 2"#,
            r#"lodestone_llm_cost_microcents_total{purpose="triage",provider_kind="anthropic",model="m",profile="p1"} 42"#,
            r#"lodestone_source_run_duration_seconds_count{source="boamp"} 1"#,
            r#"lodestone_job_wait_seconds_count{kind="source.run"} 1"#,
        ] {
            assert!(body.contains(line), "missing {line}\n{body}");
        }
    }

    #[test]
    fn queue_depth_zero_fills_statuses_and_replaces_previous_values() {
        let metrics = Metrics::new();
        metrics.set_queue_depth(&[("source.run".to_owned(), "pending".to_owned(), 4)]);
        let body = metrics.render().unwrap();
        assert!(body.contains(r#"lodestone_jobs_queue{kind="source.run",status="pending"} 4"#));
        assert!(body.contains(r#"lodestone_jobs_queue{kind="source.run",status="running"} 0"#));
        assert!(body.contains(r#"lodestone_jobs_queue{kind="source.run",status="failed"} 0"#));
        metrics.set_queue_depth(&[]);
        assert!(!metrics.render().unwrap().contains("lodestone_jobs_queue{"));
    }
}
