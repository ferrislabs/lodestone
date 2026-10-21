use std::{
    num::{NonZeroU16, NonZeroU32},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use api::internal_router::internal_router;
use axum::body::{Body, to_bytes};
use chrono::{DateTime, TimeZone, Utc};
use http::{Request, StatusCode, header::CONTENT_TYPE};
use lodestone_core::{
    ActiveProfile, Candidate, Clock, CreateProfileCommand, Job, JobKind, JobQueue, LlmError,
    LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase, NewJob, NormalizedDomain, Observation,
    ProfileId, ProfileName, Provenance, ProviderKind, RetryPolicy, SignalKind, Source, SourceError,
    SourceName, TokenPricing, TokenUsage, UnitInterval, ValidatedProfile,
    definition::{KeywordSource, ProfileDefinition},
    infrastructure::{job::postgres::PgJobQueue, llm_cache::postgres::PgLlmCache},
    jobs::{JobError, JobHandler, JobOutcome, SystemClock, WorkerPool, WorkerSettings},
    llm::gateway::CallSettings,
    triage::{CandidateSource, TriageSettings},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use telemetry::Metrics;
use tokio::sync::watch;
use tower::ServiceExt;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn domain(host: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(host).unwrap()
}

struct FakeSource;

impl Source for FakeSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        SourceName::HttpProbe
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        Ok(vec!["sso.acme.com", "auth.beta.io"])
    }

    fn evaluate(&self, item: &Self::Item, _profile: &ActiveProfile) -> Vec<Observation> {
        vec![Observation {
            domain: domain(item),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: Some("keycloak".to_owned()),
            raw: json!({ "host": item }),
            provenance: Provenance::parse(&format!("https://{item}/realms/master")).unwrap(),
            strength: UnitInterval::new(0.8).unwrap(),
            observed_at: at(50),
        }]
    }
}

struct FakeCandidates {
    hosts: Vec<&'static str>,
}

impl CandidateSource for FakeCandidates {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    fn rules<'a>(&self, profile: &'a ProfileDefinition) -> Option<&'a KeywordSource> {
        profile.sources.boamp.as_ref()
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self
            .hosts
            .iter()
            .map(|host| Candidate {
                domain: domain(host),
                source: SourceName::Boamp,
                kind: SignalKind::parse("public_tender").unwrap(),
                title: "Projet IAM pour la region".to_owned(),
                text: "Avis de marche public".to_owned(),
                provenance: Provenance::parse(&format!("https://marches.example/avis/{host}"))
                    .unwrap(),
                observed_at: at(50),
                details: json!({"idweb": host}),
            })
            .collect())
    }
}

struct Provider {
    calls: AtomicU64,
}

impl LlmProvider for Provider {
    async fn complete(&self, _request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LlmResponse {
            output: json!({
                "relevant": false,
                "reason": "off topic",
                "evidence": [],
                "contact_email": null
            }),
            model: "reported-model".to_owned(),
            usage: TokenUsage {
                input_tokens: 1_000,
                output_tokens: 100,
            },
        })
    }
}

fn triage_settings(budget_cents: u32, input_cents_per_mtok: u32) -> TriageSettings {
    TriageSettings {
        call: CallSettings {
            max_retries: 0,
            pricing: TokenPricing {
                input_cents_per_mtok,
                output_cents_per_mtok: 1_500,
            },
            base_delay: Duration::ZERO,
        },
        max_tokens: 512,
        budget_cents: NonZeroU32::new(budget_cents).unwrap(),
    }
}

struct Done;

impl JobHandler for Done {
    async fn handle(&self, _job: &Job) -> Result<JobOutcome, JobError> {
        Ok(JobOutcome::Done)
    }
}

async fn scrape(app: axum::Router) -> (StatusCode, String, String) {
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).unwrap(),
    )
}

fn job(kind: &str, run_at: DateTime<Utc>) -> NewJob {
    NewJob {
        kind: JobKind::parse(kind).unwrap(),
        payload: Value::Null,
        run_at,
        max_attempts: NonZeroU16::new(3).unwrap(),
        dedup_key: None,
    }
}

async fn process_one_job(pool: &PgPool, metrics: &Metrics) {
    let queue = Arc::new(PgJobQueue::new(pool.clone()));
    let now = Utc::now();
    queue
        .enqueue(&job("test.job", now - chrono::Duration::seconds(5)), now)
        .await
        .unwrap();
    for _ in 0..2 {
        queue
            .enqueue(&job("queued.job", now + chrono::Duration::days(1)), now)
            .await
            .unwrap();
    }
    let workers = WorkerPool::new(
        Arc::clone(&queue),
        Arc::new(Done),
        Arc::new(SystemClock),
        WorkerSettings {
            workers: NonZeroU16::new(1).unwrap(),
            lease: Duration::from_secs(30),
            poll_interval: Duration::from_millis(20),
            retry: RetryPolicy {
                base_delay: Duration::from_secs(1),
                max_delay: Duration::from_secs(10),
            },
            shutdown_grace: Duration::from_secs(1),
        },
    )
    .with_metrics(metrics.clone());
    let (stop, rx) = watch::channel(false);
    let running = tokio::spawn(workers.run(rx));
    for _ in 0..200 {
        let (succeeded,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM jobs WHERE kind = 'test.job' AND status = 'succeeded'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if succeeded == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    stop.send_replace(true);
    running.await.unwrap();
}

async fn activate_profile(usecase: &LodestoneUseCase) -> ProfileId {
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
            definition: ValidatedProfile::parse(serde_json::from_str(IAM).unwrap()).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

fn has(body: &str, line: &str) {
    assert!(
        body.lines().any(|candidate| candidate == line),
        "missing line: {line}\n{body}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_internal_endpoint_exposes_every_metric_with_its_labels(pool: PgPool) {
    let metrics = Metrics::new();
    let app = internal_router(metrics.clone(), pool.clone()).unwrap();
    let (status, content_type, before) = scrape(app.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, telemetry::CONTENT_TYPE);
    for (name, kind) in [
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
    ] {
        has(&before, &format!("# TYPE {name} {kind}"));
    }

    let usecase = LodestoneUseCase::new(pool.clone()).with_metrics(metrics.clone());
    let profile = activate_profile(&usecase).await;
    let id = profile.0.to_string();

    usecase.run_source(&FakeSource, &FixedClock).await.unwrap();

    let provider = Provider {
        calls: AtomicU64::new(0),
    };
    let cache = PgLlmCache::new(pool.clone());
    let anthropic = usecase
        .clone()
        .with_llm_provider(ProviderKind::Anthropic, "claude-test");
    let source = FakeCandidates {
        hosts: vec!["region.fr"],
    };
    for _ in 0..2 {
        anthropic
            .run_triage(
                &source,
                Some(&provider),
                &cache,
                triage_settings(100, 300),
                &FixedClock,
            )
            .await
            .unwrap();
    }
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);

    let mistral = usecase
        .clone()
        .with_llm_provider(ProviderKind::Mistral, "mistral-test");
    let exhausting = FakeCandidates {
        hosts: vec!["one.fr", "two.fr"],
    };
    let report = mistral
        .run_triage(
            &exhausting,
            Some(&provider),
            &cache,
            triage_settings(1, 1_000),
            &FixedClock,
        )
        .await
        .unwrap();
    assert!(report.budget_exhausted);

    process_one_job(&pool, &metrics).await;

    let (status, _, body) = scrape(app).await;
    assert_eq!(status, StatusCode::OK);

    has(
        &body,
        &format!(
            r#"lodestone_source_runs_total{{source="http_probe",profile="{id}",outcome="succeeded"}} 1"#
        ),
    );
    has(
        &body,
        &format!(r#"lodestone_source_items_total{{source="http_probe",profile="{id}"}} 2"#),
    );
    has(
        &body,
        &format!(
            r#"lodestone_signals_recorded_total{{source="http_probe",profile="{id}",kind="product_detected"}} 2"#
        ),
    );
    has(
        &body,
        &format!(
            r#"lodestone_source_runs_total{{source="boamp",profile="{id}",outcome="succeeded"}} 2"#
        ),
    );
    has(
        &body,
        &format!(
            r#"lodestone_source_runs_total{{source="boamp",profile="{id}",outcome="failed"}} 1"#
        ),
    );
    has(
        &body,
        r#"lodestone_source_errors_total{source="boamp",kind="budget"} 1"#,
    );
    has(
        &body,
        r#"lodestone_source_run_duration_seconds_count{source="http_probe"} 1"#,
    );

    let uncached = format!(
        r#"lodestone_llm_calls_total{{purpose="triage",provider_kind="anthropic",model="claude-test",profile="{id}",outcome="succeeded",cached="false"}} 1"#
    );
    let cached = format!(
        r#"lodestone_llm_calls_total{{purpose="triage",provider_kind="anthropic",model="claude-test",profile="{id}",outcome="succeeded",cached="true"}} 1"#
    );
    has(&body, &uncached);
    has(&body, &cached);
    has(
        &body,
        &format!(
            r#"lodestone_llm_tokens_total{{purpose="triage",provider_kind="anthropic",model="claude-test",profile="{id}",direction="input"}} 1000"#
        ),
    );
    has(
        &body,
        &format!(
            r#"lodestone_llm_tokens_total{{purpose="triage",provider_kind="anthropic",model="claude-test",profile="{id}",direction="output"}} 100"#
        ),
    );
    has(
        &body,
        &format!(
            r#"lodestone_llm_cost_microcents_total{{purpose="triage",provider_kind="anthropic",model="claude-test",profile="{id}"}} {}"#,
            1_000 * 300 + 100 * 1_500
        ),
    );
    has(
        &body,
        &format!(
            r#"lodestone_llm_cost_microcents_total{{purpose="triage",provider_kind="mistral",model="mistral-test",profile="{id}"}} {}"#,
            1_000 * 1_000 + 100 * 1_500
        ),
    );
    has(
        &body,
        &format!(r#"lodestone_llm_budget_exhausted_total{{purpose="triage",profile="{id}"}} 1"#),
    );

    has(
        &body,
        r#"lodestone_jobs_total{kind="test.job",outcome="done"} 1"#,
    );
    has(
        &body,
        r#"lodestone_job_duration_seconds_count{kind="test.job"} 1"#,
    );
    has(
        &body,
        r#"lodestone_job_wait_seconds_count{kind="test.job"} 1"#,
    );
    has(
        &body,
        r#"lodestone_jobs_queue{kind="queued.job",status="pending"} 2"#,
    );
    has(
        &body,
        r#"lodestone_jobs_queue{kind="queued.job",status="running"} 0"#,
    );
    has(
        &body,
        r#"lodestone_jobs_queue{kind="queued.job",status="failed"} 0"#,
    );

    let forbidden = [
        "acme",
        "beta.io",
        "region.fr",
        "one.fr",
        "two.fr",
        "marches",
        "://",
        "http_probe/",
    ];
    for line in body.lines().filter(|line| !line.starts_with('#')) {
        for needle in forbidden {
            assert!(!line.contains(needle), "label leaks `{needle}`: {line}");
        }
    }
}
