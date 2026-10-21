mod support;

use std::{
    collections::{HashMap, VecDeque},
    num::{NonZeroU32, NonZeroUsize},
    slice::from_ref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Duration as Delta, NaiveDate, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, BODACC, COMPANY_RESEARCH, Clock, CompanyBuilder, CompanyPage,
    CreateProfileCommand, DECP, DossierBuild, DossierState, EcosystemFinding, EcosystemModule,
    EvaluationStage, EvaluationStatus, EventStatus, GenerationParams, Inspection, InspectionTarget,
    Lead, LeadQuery, LeadRepository, LinkRejection, LlmError, LlmProvider, LlmRequest, LlmResponse,
    LodestoneUseCase, ModuleRunner, ModuleStatus, NormalizedDomain, Observation, OfficialData,
    PageReview, PageVerifier, ProfileId, ProfileName, Provenance, ProviderDefinition, ProviderKind,
    RequalifyPolicy, RoundTrigger, SignalKind, Siren, SourceError, SourceName, Stage, StageReason,
    StageStatus, Tier, TimelineEvent, TimelineStep, TokenPricing, TokenUsage, UnconfirmedLinks,
    UnitInterval, ValidatedProfile,
    company::{CompanyRunReport, CompanyRunSettings, PageRecheckReport},
    definition::AgentId,
    ecosystem::{InspectionReport, InspectionRunSettings},
    evaluation::{EvaluationSettings, ProviderBuilder},
    infrastructure::{
        crypto::AesGcmCipher, lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache,
        postgres::with_tx,
    },
    llm::CreateLlmProvider,
    sources::{LeadCreation, Skipped},
    timeline_kind,
};
use serde_json::{Value, json};
use sqlx::{PgPool, query, query_as, query_scalar};
use support::{at, domain};
use tokio::time::{Instant, sleep};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const MODEL: &str = "model-a";
const PAGE_URL: &str = "https://acme.com/a-propos";
const PAGE_TEXT: &str = "Nous fabriquons des pieces mecaniques depuis 2010";
const DEBOUNCE: Duration = Duration::from_secs(120);
const NEXT_UTC_DAY: i64 = 56_600;
const NINETY_DAYS: Duration = Duration::from_secs(90 * 86_400);

#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

struct FakeModule(EcosystemModule);

struct ChangingModule(EcosystemModule, Arc<AtomicI64>);

#[derive(Default)]
struct Visits {
    running: Mutex<HashMap<String, u32>>,
    peaks: Mutex<(u32, u32)>,
    spans: Mutex<Vec<(EcosystemModule, String, Instant, Instant)>>,
}

impl Visits {
    fn enter(&self, domain: &str) {
        let mut running = self.running.lock().unwrap();
        *running.entry(domain.to_owned()).or_default() += 1;
        let busy = u32::try_from(running.values().filter(|count| **count > 0).count()).unwrap();
        let mut peaks = self.peaks.lock().unwrap();
        peaks.0 = peaks.0.max(busy);
        peaks.1 = peaks.1.max(running[domain]);
    }

    fn leave(&self, domain: &str) {
        *self.running.lock().unwrap().get_mut(domain).unwrap() -= 1;
    }

    fn span(&self, module: EcosystemModule, domain: &str) -> (Instant, Instant) {
        self.spans
            .lock()
            .unwrap()
            .iter()
            .find(|(seen, host, _, _)| *seen == module && host == domain)
            .map(|(_, _, start, end)| (*start, *end))
            .unwrap()
    }
}

struct PacedModule {
    module: EcosystemModule,
    visits: Arc<Visits>,
    limited: bool,
}

impl ModuleRunner for PacedModule {
    fn module(&self) -> EcosystemModule {
        self.module
    }

    async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        let domain = target.domain.as_str();
        if self.limited {
            return Err(SourceError::RateLimited {
                retry_after: Duration::from_secs(60),
            });
        }
        let start = Instant::now();
        self.visits.enter(domain);
        sleep(Duration::from_millis(40)).await;
        self.visits.leave(domain);
        self.visits.spans.lock().unwrap().push((
            self.module,
            domain.to_owned(),
            start,
            Instant::now(),
        ));
        Ok(Inspection::Found(vec![EcosystemFinding::new(
            self.module,
            "fact",
            "Fait observé",
            format!("FACT-OF-{}", self.module.as_str()),
            format!("https://{domain}/{}", self.module.as_str()),
            target.now,
        )]))
    }
}

struct RecordingModule(Arc<Mutex<Vec<String>>>);

impl ModuleRunner for RecordingModule {
    fn module(&self) -> EcosystemModule {
        EcosystemModule::DnsEmail
    }

    async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        self.0
            .lock()
            .unwrap()
            .push(target.domain.as_str().to_owned());
        Ok(Inspection::Found(Vec::new()))
    }
}

impl ModuleRunner for ChangingModule {
    fn module(&self) -> EcosystemModule {
        self.0
    }

    async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        let round = self.1.load(Ordering::SeqCst);
        Ok(Inspection::Found(vec![EcosystemFinding::new(
            self.0,
            "fact",
            "Fait observé",
            format!("ROUND-{round}"),
            format!("https://{}/{}", target.domain, self.0.as_str()),
            target.now,
        )]))
    }
}

impl ModuleRunner for FakeModule {
    fn module(&self) -> EcosystemModule {
        self.0
    }

    async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        Ok(Inspection::Found(vec![EcosystemFinding::new(
            self.0,
            "fact",
            "Fait observé",
            format!("FACT-OF-{}", self.0.as_str()),
            format!("https://{}/{}", target.domain, self.0.as_str()),
            target.now,
        )
        .with_details(match self.0 {
            EcosystemModule::LegalLife => {
                json!({"date": "2026-09-10", "court": "Greffe du tribunal de commerce de Lyon"})
            }
            EcosystemModule::PublicContracts => {
                json!({"date": "2026-08-10", "buyer": "Commune de Lorient", "amount_eur": 1200.5, "procedure": "Procédure adaptée"})
            }
            _ => Value::Null,
        })]))
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        at(1_000 + self.0.load(Ordering::SeqCst))
    }
}

#[derive(Debug, Clone)]
struct Seen {
    aggregator: bool,
    scoring: bool,
    documents: Vec<(String, String)>,
    system: String,
    instruction: String,
}

struct Shared {
    seen: Mutex<Vec<Seen>>,
    aggregator_reply: Mutex<Option<Value>>,
}

struct Scripted {
    shared: Arc<Shared>,
}

struct FakeBuilder {
    shared: Arc<Shared>,
}

impl ProviderBuilder for FakeBuilder {
    type Provider = Scripted;

    fn build(&self, _: &ProviderDefinition) -> Result<Scripted, LlmError> {
        Ok(Scripted {
            shared: Arc::clone(&self.shared),
        })
    }
}

fn answers_from(schema: &Value, quote: &str) -> Value {
    let mut answers = serde_json::Map::new();
    let questions = schema["properties"]["answers"]["properties"]
        .as_object()
        .unwrap();
    for (id, question) in questions {
        let values = question["properties"]["choice"]["enum"].as_array().unwrap();
        let probabilities: serde_json::Map<String, Value> = values
            .iter()
            .enumerate()
            .map(|(position, value)| {
                (
                    value.as_str().unwrap().to_owned(),
                    json!(if position == 0 { 1.0 } else { 0.0 }),
                )
            })
            .collect();
        answers.insert(
            id.clone(),
            json!({"type": "choice", "choice": values[0], "evidence": quote, "confidence": 0.9, "probabilities": probabilities}),
        );
    }
    json!({"summary": "Observations de l'agent sur le lead.", "answers": answers})
}

fn summary_quoting(index: u32, quote: &str) -> Value {
    json!({
        "summary": "Le domaine expose un Keycloak ancien.",
        "verdict_narrative": "Les agents jugent le lead pertinent.",
        "approach_angle": "Proposer une migration accompagnée.",
        "evidence": [{"document_index": index, "quote": quote}],
        "draft_message": {"subject": "Votre Keycloak", "body": "Bonjour, un échange de 20 minutes ?"}
    })
}

impl LlmProvider for Scripted {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        let scoring = request.schema["properties"].get("criteria").is_some();
        let aggregator = !scoring && request.schema["properties"].get("answers").is_none();
        self.shared.seen.lock().unwrap().push(Seen {
            aggregator,
            scoring,
            documents: request
                .documents
                .iter()
                .map(|document| (document.source.to_owned(), document.content.to_owned()))
                .collect(),
            system: request.system.to_owned(),
            instruction: request.instruction.to_owned(),
        });
        let first_line = request
            .documents
            .first()
            .and_then(|document| document.content.lines().next())
            .unwrap_or_default();
        let output = if scoring {
            support::scoring_answer(request.schema, first_line, false)
        } else if aggregator {
            self.shared
                .aggregator_reply
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| summary_quoting(0, "Keycloak-http_probe"))
        } else {
            answers_from(request.schema, first_line)
        };
        Ok(LlmResponse {
            output,
            model: MODEL.to_owned(),
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 0,
            },
        })
    }
}

struct FakeCompany {
    calls: Mutex<Vec<String>>,
    next: Mutex<VecDeque<DossierBuild>>,
}

impl FakeCompany {
    fn new(builds: Vec<DossierBuild>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            next: Mutex::new(builds.into()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl CompanyBuilder for FakeCompany {
    async fn build(&self, domain: &NormalizedDomain) -> Result<DossierBuild, SourceError> {
        self.calls.lock().unwrap().push(domain.as_str().to_owned());
        Ok(self
            .next
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(complete_build))
    }
}

fn official() -> OfficialData {
    OfficialData {
        siren: Siren::parse("123456782").unwrap(),
        legal_name: "ACME INDUSTRIE".to_owned(),
        naf_code: Some("28.29B".to_owned()),
        naf_label: Some("Industrie manufacturière".to_owned()),
        headcount_code: Some("12".to_owned()),
        headcount_range: Some("20 à 49 salariés".to_owned()),
        head_office_city: Some("LYON".to_owned()),
        head_office_postcode: Some("69002".to_owned()),
        head_office_country: Some("France".to_owned()),
        head_office_country_code: Some("FR".to_owned()),
        created_on: NaiveDate::from_ymd_opt(2010, 5, 4),
    }
}

fn pages(text: &str) -> Vec<CompanyPage> {
    vec![
        CompanyPage {
            url: "https://acme.com/".to_owned(),
            text: "Bienvenue chez ACME".to_owned(),
        },
        CompanyPage {
            url: PAGE_URL.to_owned(),
            text: text.to_owned(),
        },
    ]
}

fn complete_build() -> DossierBuild {
    DossierBuild {
        siren: Some(official().siren),
        official: Some(official()),
        pages: pages(PAGE_TEXT),
        reachable: true,
        failure: None,
        unconfirmed: UnconfirmedLinks::default(),
    }
}

fn unreachable_build() -> DossierBuild {
    DossierBuild {
        siren: None,
        official: None,
        pages: Vec::new(),
        reachable: false,
        failure: Some("robots.txt disallows the home page".to_owned()),
        unconfirmed: UnconfirmedLinks::default(),
    }
}

struct FakeVerifier {
    rejected: Vec<(String, LinkRejection)>,
    rate_limited: bool,
    calls: Mutex<Vec<Vec<String>>>,
}

impl FakeVerifier {
    fn rejecting(rejected: &[(&str, LinkRejection)]) -> Self {
        Self {
            rejected: rejected
                .iter()
                .map(|(url, reason)| ((*url).to_owned(), *reason))
                .collect(),
            rate_limited: false,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl PageVerifier for FakeVerifier {
    async fn review(&self, pages: &[CompanyPage]) -> Result<PageReview, SourceError> {
        self.calls
            .lock()
            .unwrap()
            .push(pages.iter().map(|page| page.url.clone()).collect());
        if self.rate_limited {
            return Err(SourceError::RateLimited {
                retry_after: Duration::from_secs(30),
            });
        }
        Ok(PageReview {
            rejected: self
                .rejected
                .iter()
                .filter(|(url, _)| pages.iter().any(|page| &page.url == url))
                .cloned()
                .collect(),
        })
    }
}

struct Harness {
    pool: PgPool,
    usecase: LodestoneUseCase,
    cipher: AesGcmCipher,
    shared: Arc<Shared>,
    clock: TestClock,
}

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QkI=", "QkJC".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

fn evaluation() -> EvaluationSettings {
    EvaluationSettings {
        max_tokens: 512,
        budget_cents: NonZeroU32::new(100).unwrap(),
        batch_size: 10,
        base_delay: Duration::ZERO,
        concurrency: NonZeroUsize::MIN,
    }
}

fn research() -> CompanyRunSettings {
    CompanyRunSettings {
        batch_size: 10,
        refresh_interval: NINETY_DAYS,
    }
}

fn question(id: &str) -> Value {
    json!({
        "id": id,
        "wording": "Pertinent ?",
        "weight": 5,
        "options": [
            {"value": "forte", "label": "Forte", "score": 1.0},
            {"value": "faible", "label": "Faible", "score": 0.0},
            {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
        ]
    })
}

fn definition() -> Value {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["qualification_agents"] = json!([{
        "id": "one",
        "name": "one",
        "description": "DESCRIPTION-OF-one",
        "enabled": true,
        "provider_id": null,
        "system_prompt": "PROMPT-OF-one",
        "samples": 1,
        "questions": [question("q"), question("q2")]
    }]);
    definition["aggregator"]["system_prompt"] = json!("AGGREGATOR-PROMPT");
    definition["aggregator"]["provider_id"] = Value::Null;
    definition
}

impl Harness {
    fn new(pool: PgPool) -> Self {
        Self {
            usecase: LodestoneUseCase::new(pool.clone()),
            pool,
            cipher: cipher(),
            clock: TestClock(Arc::new(AtomicI64::new(0))),
            shared: Arc::new(Shared {
                seen: Mutex::new(Vec::new()),
                aggregator_reply: Mutex::new(None),
            }),
        }
    }

    fn requalifying(mut self, debounce: Duration, max_per_day: u32) -> Self {
        self.usecase = self
            .usecase
            .with_requalify_policy(RequalifyPolicy::from_std(debounce, max_per_day));
        self
    }

    async fn listed_evaluation(&self, profile: ProfileId, lead: &Lead) -> EvaluationStatus {
        let listing = self
            .usecase
            .list_leads(
                profile,
                LeadQuery {
                    limit: 50,
                    ..LeadQuery::default()
                },
            )
            .await
            .unwrap();
        listing.evaluations.get(&lead.id).cloned().unwrap()
    }

    async fn observe_kind(&self, host: &str, source: SourceName, kind: &str, seconds: i64) {
        let active = self.active().await;
        self.usecase
            .record_observations(
                &active,
                source,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse(kind).unwrap(),
                    value: Some(format!("{kind}-{host}")),
                    raw: json!({}),
                    provenance: Provenance::parse(&format!("https://{host}/{kind}")).unwrap(),
                    strength: UnitInterval::new(0.3).unwrap(),
                    observed_at: at(seconds),
                    agent: None,
                }],
            )
            .await
            .unwrap();
    }

    async fn provider(&self, key: &str, status: &str, replace: bool) {
        let sql = if replace {
            "INSERT INTO source_provider_configurations (provider_key, settings, status, created_at, updated_at) VALUES ($1, '{}'::jsonb, $2, now(), now()) ON CONFLICT (provider_key) DO UPDATE SET status = EXCLUDED.status"
        } else {
            "INSERT INTO source_provider_configurations (provider_key, settings, status, created_at, updated_at) VALUES ($1, '{}'::jsonb, $2, now(), now()) ON CONFLICT (provider_key) DO NOTHING"
        };
        query(sql)
            .bind(key)
            .bind(status)
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn inspect(&self, modules: &[EcosystemModule]) {
        let runners: Vec<FakeModule> = modules.iter().copied().map(FakeModule).collect();
        self.inspect_with(&runners).await;
    }

    async fn inspect_with<R: ModuleRunner>(&self, runners: &[R]) -> u64 {
        for key in [COMPANY_RESEARCH, BODACC, DECP] {
            self.provider(key, "ready", false).await;
        }
        self.usecase
            .inspect_due_domains(
                runners,
                InspectionRunSettings {
                    batch_size: 10,
                    priority_batch_size: 30,
                    concurrency: NonZeroUsize::MIN,
                },
                &self.clock,
            )
            .await
            .unwrap()
            .changed
    }

    async fn inspect_at_once<R: ModuleRunner>(
        &self,
        runners: &[R],
        concurrency: usize,
    ) -> InspectionReport {
        for key in [COMPANY_RESEARCH, BODACC, DECP] {
            self.provider(key, "ready", false).await;
        }
        self.usecase
            .inspect_due_domains(
                runners,
                InspectionRunSettings {
                    batch_size: 10,
                    priority_batch_size: 30,
                    concurrency: NonZeroUsize::new(concurrency).unwrap(),
                },
                &self.clock,
            )
            .await
            .unwrap()
    }

    async fn inspect_batches(&self, runner: &RecordingModule, priority: u32, refresh: u32) {
        self.usecase
            .inspect_due_domains(
                from_ref(runner),
                InspectionRunSettings {
                    batch_size: refresh,
                    priority_batch_size: priority,
                    concurrency: NonZeroUsize::MIN,
                },
                &self.clock,
            )
            .await
            .unwrap();
    }

    async fn created_ago(&self, host: &str, seconds: i64) {
        query("UPDATE leads SET created_at = $2 WHERE domain = $1")
            .bind(host)
            .bind(self.clock.now() - Delta::seconds(seconds))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn modules_due_again(&self) {
        query("UPDATE lead_ecosystem_modules SET next_run_at = $1")
            .bind(self.clock.now())
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn record_host(&self, host: &str, agent: &str) -> Skipped {
        let active = self.active().await;
        let countries = active
            .revision
            .definition
            .definition()
            .agents
            .iter()
            .find(|candidate| candidate.id().as_str() == agent)
            .and_then(|candidate| candidate.host_countries())
            .unwrap_or_default()
            .to_vec();
        self.usecase
            .record_observations_with(
                &active,
                SourceName::CertificateTransparency,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("certificate_seen").unwrap(),
                    value: Some(format!("auth.{host}")),
                    raw: json!({}),
                    provenance: Provenance::parse(&format!("https://crt.sh/?q=auth.{host}"))
                        .unwrap(),
                    strength: UnitInterval::new(0.3).unwrap(),
                    observed_at: at(10),
                    agent: Some(AgentId::new(agent)),
                }],
                self.clock.now(),
                LeadCreation::Hosts(&countries),
            )
            .await
            .unwrap()
    }

    async fn record_search(&self, host: &str, agent: &str) -> Skipped {
        let active = self.active().await;
        self.usecase
            .record_observations_with(
                &active,
                SourceName::Boamp,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("public_tender").unwrap(),
                    value: Some(format!("tender-{host}")),
                    raw: json!({}),
                    provenance: Provenance::parse(&format!("https://{host}/tender")).unwrap(),
                    strength: UnitInterval::new(0.6).unwrap(),
                    observed_at: at(20),
                    agent: Some(AgentId::new(agent)),
                }],
                self.clock.now(),
                LeadCreation::Create,
            )
            .await
            .unwrap()
    }

    async fn exclusion(&self, profile: ProfileId, host: &str) -> Option<String> {
        query_scalar("SELECT excluded_reason FROM leads WHERE profile_id = $1 AND domain = $2")
            .bind(profile.0)
            .bind(host)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    fn scoring_calls(&self) -> usize {
        self.shared
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.scoring)
            .count()
    }

    async fn daily_count(&self, lead: &Lead) -> i32 {
        query_scalar("SELECT daily_count FROM lead_requalifications WHERE lead_id = $1")
            .bind(lead.id.0)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    fn every_call(&self) -> Vec<Seen> {
        self.shared.seen.lock().unwrap().clone()
    }

    async fn defaults(&self) {
        for tier in [Tier::Triage, Tier::Qualification] {
            let provider = self
                .usecase
                .create_llm_provider(
                    CreateLlmProvider {
                        name: format!("model-{}", tier.as_str()),
                        kind: ProviderKind::OpenAiCompatible,
                        base_url: "https://llm.example/v1".parse().unwrap(),
                        model: MODEL.to_owned(),
                        timeout: Duration::from_secs(30),
                        max_retries: 0,
                        max_concurrency: None,
                        pricing: TokenPricing {
                            input_cents_per_mtok: 0,
                            output_cents_per_mtok: 0,
                        },
                        api_key: None,
                        reasoning_effort: None,
                        generation: GenerationParams::default(),
                    },
                    &self.cipher,
                )
                .await
                .unwrap()
                .id;
            self.usecase.assign_llm_tier(tier, provider).await.unwrap();
        }
    }

    async fn profile(&self) -> ProfileId {
        self.profile_named("iam").await
    }

    async fn profile_named(&self, name: &str) -> ProfileId {
        let id = self
            .usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse(name).unwrap(),
                definition: ValidatedProfile::parse(definition()).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        self.usecase.set_profile_active(id, true).await.unwrap();
        id
    }

    async fn active(&self) -> ActiveProfile {
        self.usecase.list_active_profiles().await.unwrap().remove(0)
    }

    async fn observe(&self, host: &str, source: SourceName, seconds: i64) {
        let active = self.active().await;
        self.observe_in(&active, host, source, seconds).await;
    }

    async fn observe_in(
        &self,
        active: &ActiveProfile,
        host: &str,
        source: SourceName,
        seconds: i64,
    ) {
        self.usecase
            .record_observations(
                active,
                source,
                &[Observation {
                    domain: domain(host),
                    kind: SignalKind::parse("product_detected").unwrap(),
                    value: Some(format!("keycloak-{}", source.as_str())),
                    raw: json!({"version": "21.1"}),
                    provenance: Provenance::parse(&format!("https://{host}/{}", source.as_str()))
                        .unwrap(),
                    strength: UnitInterval::new(0.9).unwrap(),
                    observed_at: at(seconds),
                    agent: None,
                }],
            )
            .await
            .unwrap();
    }

    async fn created_at_clock_zero(&self) {
        sqlx::query("UPDATE leads SET created_at = $1")
            .bind(at(1_000))
            .execute(&self.pool)
            .await
            .unwrap();
    }

    fn set_time(&self, seconds: i64) {
        self.clock.0.store(seconds, Ordering::SeqCst);
    }

    async fn lead(&self, profile: ProfileId, host: &str) -> Lead {
        with_tx(&self.pool, async |shared| {
            PgLeadRepository::new(&shared)
                .find_by_domain(profile, &domain(host))
                .await
        })
        .await
        .unwrap()
        .unwrap()
    }

    async fn research(&self, builder: &FakeCompany) -> CompanyRunReport {
        self.usecase
            .listen_company_research(builder, research(), &self.clock)
            .await
            .unwrap()
    }

    async fn recheck(&self, verifier: &FakeVerifier) -> PageRecheckReport {
        self.usecase
            .recheck_company_pages(verifier, 10, &self.clock)
            .await
            .unwrap()
    }

    async fn forget_page_checks(&self) {
        sqlx::query("UPDATE company_dossiers SET pages_checked_at = NULL")
            .execute(&self.pool)
            .await
            .unwrap();
    }

    async fn qualify(&self) -> u64 {
        let leads = self
            .usecase
            .qualify_due_leads(
                &FakeBuilder {
                    shared: Arc::clone(&self.shared),
                },
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                evaluation(),
                &self.clock,
            )
            .await
            .unwrap()
            .leads;
        self.usecase
            .score_due_leads(
                &FakeBuilder {
                    shared: Arc::clone(&self.shared),
                },
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                evaluation(),
                &self.clock,
            )
            .await
            .unwrap();
        leads
    }

    async fn summarise(&self) -> u64 {
        self.usecase
            .summarise_due_leads(
                &FakeBuilder {
                    shared: Arc::clone(&self.shared),
                },
                &self.cipher,
                &PgLlmCache::new(self.pool.clone()),
                evaluation(),
                &self.clock,
            )
            .await
            .unwrap()
            .leads
    }

    fn calls(&self, aggregator: bool) -> Vec<Seen> {
        self.shared
            .seen
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.aggregator == aggregator && !call.scoring)
            .cloned()
            .collect()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_listener_builds_a_dossier_for_a_new_lead_and_again_after_ninety_days(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.observe("other.fr", SourceName::HttpProbe, 11).await;
    let builder = FakeCompany::new(Vec::new());

    let first = harness.research(&builder).await;

    assert_eq!(first.domains, 2);
    assert_eq!(first.changed, 2);
    assert_eq!(first.backlog, 0);
    let mut called = builder.calls();
    called.sort();
    assert_eq!(called, ["acme.com", "other.fr"]);
    let lead = harness.lead(profile, "acme.com").await;
    let stored = harness
        .usecase
        .company_dossier(&lead.domain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, DossierState::Complete);
    assert_eq!(stored.official.unwrap().legal_name, "ACME INDUSTRIE");
    assert_eq!(stored.pages.len(), 2);
    assert_eq!(
        stored.next_attempt_at,
        at(1_000) + chrono::Duration::days(90)
    );

    assert_eq!(harness.research(&builder).await.domains, 0);
    harness.set_time(89 * 86_400);
    assert_eq!(harness.research(&builder).await.domains, 0);
    assert_eq!(builder.calls().len(), 2);

    harness.set_time(91 * 86_400);
    let later = harness.research(&builder).await;
    assert_eq!(later.domains, 2);
    assert_eq!(later.changed, 0);
    assert_eq!(builder.calls().len(), 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_domains_of_active_profiles_are_researched(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness
        .usecase
        .set_profile_active(profile, false)
        .await
        .unwrap();
    let builder = FakeCompany::new(Vec::new());

    assert_eq!(harness.research(&builder).await.domains, 0);
    assert!(builder.calls().is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refresh_request_forces_a_rebuild_of_a_fresh_dossier(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(Vec::new());
    harness.research(&builder).await;
    assert_eq!(harness.research(&builder).await.domains, 0);
    let lead = harness.lead(profile, "acme.com").await;

    let requested = harness
        .usecase
        .request_company_refresh_at(lead.id, at(1_000))
        .await
        .unwrap();
    assert_eq!(requested.state, DossierState::Complete);
    assert_eq!(requested.next_attempt_at, at(1_000));

    assert_eq!(harness.research(&builder).await.domains, 1);
    assert_eq!(builder.calls().len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refresh_request_for_a_lead_without_a_dossier_creates_a_pending_one(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let lead = harness.lead(profile, "acme.com").await;

    let requested = harness
        .usecase
        .request_company_refresh_at(lead.id, at(1_000))
        .await
        .unwrap();

    assert_eq!(requested.state, DossierState::Pending);
    assert!(!requested.has_content());
    let unknown = lodestone_core::LeadId(uuid::Uuid::now_v7());
    assert!(
        harness
            .usecase
            .request_company_refresh(unknown)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unreachable_site_backs_off_and_never_blocks_the_loop(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(vec![unreachable_build()]);

    let report = harness.research(&builder).await;

    assert_eq!(report.domains, 1);
    assert_eq!(report.failed, 1);
    assert_eq!(report.changed, 0);
    let lead = harness.lead(profile, "acme.com").await;
    let stored = harness
        .usecase
        .company_dossier(&lead.domain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.state, DossierState::Unreachable);
    assert_eq!(stored.attempts, 1);
    assert_eq!(
        stored.next_attempt_at,
        at(1_000) + chrono::Duration::minutes(1)
    );
    assert!(stored.error.unwrap().contains("robots"));

    harness.set_time(30);
    assert_eq!(harness.research(&builder).await.domains, 0);
    harness.set_time(61);
    assert_eq!(harness.research(&builder).await.domains, 1);
    assert_eq!(builder.calls().len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_is_qualified_at_once_without_waiting_for_its_dossier_or_inspection(pool: PgPool) {
    let harness = Harness::new(pool).requalifying(DEBOUNCE, 3);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.created_at_clock_zero().await;
    let lead = harness.lead(profile, "acme.com").await;
    assert_eq!(
        harness.listed_evaluation(profile, &lead).await.stage,
        EvaluationStage::NotEvaluated
    );

    assert_eq!(harness.qualify().await, 1);
    let calls = harness.calls(false);
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0]
            .documents
            .iter()
            .all(|(source, _)| !source.contains("acme.com/a-propos"))
    );
    let status = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(status.stage, EvaluationStage::InspectionRunning);
    assert_eq!((status.modules_done, status.modules_available), (0, 4));
    assert_eq!(status.evaluated_at, Some(harness.clock.now()));
    assert_eq!(harness.qualify().await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_dossier_without_content_does_not_requalify(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(vec![unreachable_build()]);

    assert_eq!(harness.qualify().await, 1);
    harness.set_time(10);
    harness.research(&builder).await;
    assert_eq!(harness.qualify().await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_dossier_makes_the_agents_due_again_and_the_prompts_carry_it(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.created_at_clock_zero().await;

    harness.set_time(601);
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    assert_eq!(harness.qualify().await, 0);
    assert_eq!(harness.calls(false).len(), 1);

    let builder = FakeCompany::new(Vec::new());
    harness.set_time(700);
    harness.research(&builder).await;

    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    let agent_calls = harness.calls(false);
    assert_eq!(agent_calls.len(), 2);
    let with_dossier = &agent_calls[1];
    let official = with_dossier
        .documents
        .iter()
        .find(|(source, _)| source.starts_with("https://recherche-entreprises.api.gouv.fr/search"))
        .unwrap();
    assert!(official.1.contains("- legal name: ACME INDUSTRIE"));
    assert!(official.1.contains("- headcount range: 20 à 49 salariés"));
    let page = with_dossier
        .documents
        .iter()
        .find(|(source, _)| source == PAGE_URL)
        .unwrap();
    assert!(page.1.contains(PAGE_TEXT));
    assert!(with_dossier.system.contains("Company dossier"));
    let lead_document = with_dossier
        .documents
        .iter()
        .find(|(source, _)| source == "lead")
        .unwrap();
    assert!(lead_document.1.contains(
        "- probable country, from the head office in the official company register: France (FR)"
    ));
    assert!(lead_document.1.contains(
        "- official data (French company register, SIREN 123456782): legal name ACME INDUSTRIE"
    ));

    let aggregator_calls = harness.calls(true);
    assert_eq!(aggregator_calls.len(), 2);
    let last = &aggregator_calls[1];
    assert_eq!(
        last.documents[1].0,
        "https://recherche-entreprises.api.gouv.fr/search?q=123456782"
    );
    assert!(
        last.instruction
            .contains("Documents 0 to 0 are the signals of the lead.")
    );
    assert!(
        last.instruction
            .contains("Documents 1 to 3 are the company dossier")
    );
    assert_eq!(harness.qualify().await, 0);
    let lead = harness.lead(profile, "acme.com").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.company.unwrap().state, DossierState::Complete);
    let country = detail.facts.probable_country.unwrap();
    assert_eq!(country.code, "FR");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_identical_rebuild_does_not_make_the_agents_due_again(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(Vec::new());
    harness.research(&builder).await;
    assert_eq!(harness.qualify().await, 1);

    harness.set_time(91 * 86_400);
    let report = harness.research(&builder).await;

    assert_eq!(report.domains, 1);
    assert_eq!(report.changed, 0);
    assert_eq!(harness.qualify().await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_changed_dossier_requalifies_a_lead_that_used_the_previous_one(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(vec![complete_build(), {
        let mut changed = complete_build();
        changed.pages = pages("Nous avons déménagé et changé de métier");
        changed
    }]);
    harness.research(&builder).await;
    assert_eq!(harness.qualify().await, 1);

    harness.set_time(91 * 86_400);
    assert_eq!(harness.research(&builder).await.changed, 1);

    assert_eq!(harness.qualify().await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_aggregator_evidence_may_quote_the_company_pages(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let builder = FakeCompany::new(Vec::new());
    harness.research(&builder).await;
    *harness.shared.aggregator_reply.lock().unwrap() = Some(summary_quoting(3, "depuis 2010"));

    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);

    let lead = harness.lead(profile, "acme.com").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    let assessment = &detail.assessments[0];
    assert!(assessment.evidence_verified);
    assert_eq!(assessment.evidence.len(), 1);
    assert_eq!(assessment.evidence[0].quote, "depuis 2010");
    assert_eq!(assessment.evidence[0].provenance.as_str(), PAGE_URL);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_of_a_domain_share_one_dossier_across_profiles(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    harness.profile_named("iam-two").await;
    for active in harness.usecase.list_active_profiles().await.unwrap() {
        harness
            .observe_in(&active, "acme.com", SourceName::HttpProbe, 10)
            .await;
    }
    let builder = FakeCompany::new(Vec::new());

    let report = harness.research(&builder).await;

    assert_eq!(report.domains, 1);
    assert_eq!(builder.calls(), ["acme.com"]);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM company_dossiers")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let leads: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM leads")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(leads, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn findings_of_several_modules_give_one_debounced_requalification(pool: PgPool) {
    let harness = Harness::new(pool).requalifying(DEBOUNCE, 3);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    assert_eq!(harness.qualify().await, 1);
    let lead = harness.lead(profile, "mairie.fr").await;

    harness.set_time(10);
    harness.inspect(&[EcosystemModule::WebsiteStack]).await;
    harness.set_time(60);
    harness
        .inspect(&[EcosystemModule::DnsEmail, EcosystemModule::HiringActivity])
        .await;
    let planned = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(planned.stage, EvaluationStage::RequalificationPlanned);
    assert_eq!(planned.requalify_at, Some(at(1_180)));
    assert!(!planned.requalify_forced);
    let changed: Vec<EcosystemModule> = planned
        .changed_modules
        .iter()
        .map(|state| state.module)
        .collect();
    assert_eq!(
        changed,
        [
            EcosystemModule::WebsiteStack,
            EcosystemModule::DnsEmail,
            EcosystemModule::HiringActivity
        ]
    );

    harness.set_time(150);
    assert_eq!(harness.qualify().await, 0);
    harness.set_time(181);
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.qualify().await, 0);
    assert_eq!(harness.calls(false).len(), 2);
    let after = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(after.stage, EvaluationStage::InspectionRunning);
    assert_eq!((after.modules_done, after.modules_available), (3, 7));
    assert!(after.changed_modules.is_empty());
    assert_eq!(harness.daily_count(&lead).await, 1);

    harness.set_time(200);
    harness.modules_due_again().await;
    harness
        .inspect(&[
            EcosystemModule::WebsiteStack,
            EcosystemModule::DnsEmail,
            EcosystemModule::HiringActivity,
        ])
        .await;
    harness.set_time(1_000);
    assert_eq!(harness.qualify().await, 0);
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(
        detail.evaluation,
        harness.listed_evaluation(profile, &lead).await
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_daily_cap_holds_automatic_requalifications_but_not_a_manual_one(pool: PgPool) {
    let harness = Harness::new(pool).requalifying(Duration::ZERO, 2);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    assert_eq!(harness.qualify().await, 1);
    let lead = harness.lead(profile, "mairie.fr").await;
    let round = Arc::new(AtomicI64::new(0));
    let runners = [ChangingModule(
        EcosystemModule::WebsiteStack,
        Arc::clone(&round),
    )];

    for step in 1..=2 {
        round.store(step, Ordering::SeqCst);
        harness.set_time(step * 100);
        harness.modules_due_again().await;
        assert_eq!(harness.inspect_with(&runners).await, 1);
        assert_eq!(harness.qualify().await, 1, "round {step}");
    }
    assert_eq!(harness.daily_count(&lead).await, 2);

    round.store(3, Ordering::SeqCst);
    harness.set_time(300);
    harness.modules_due_again().await;
    assert_eq!(harness.inspect_with(&runners).await, 1);
    assert_eq!(harness.qualify().await, 0);
    let capped = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(capped.stage, EvaluationStage::RequalificationPlanned);
    assert_eq!(capped.requalify_at, Some(at(1_000 + NEXT_UTC_DAY)));

    harness.set_time(310);
    harness
        .usecase
        .request_lead_reevaluation_at(lead.id, harness.clock.now())
        .await
        .unwrap();
    let manual = harness.listed_evaluation(profile, &lead).await;
    assert!(manual.requalify_forced);
    assert_eq!(manual.requalify_at, Some(at(1_310)));
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.qualify().await, 0);
    assert_eq!(harness.daily_count(&lead).await, 2);

    round.store(4, Ordering::SeqCst);
    harness.set_time(NEXT_UTC_DAY + 10);
    harness.modules_due_again().await;
    assert_eq!(harness.inspect_with(&runners).await, 1);
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.daily_count(&lead).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_module_whose_provider_is_not_ready_is_skipped_and_left_out_of_the_progress(
    pool: PgPool,
) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    harness.provider(BODACC, "failing", true).await;

    harness.inspect(&EcosystemModule::AVAILABLE).await;

    let lead = harness.lead(profile, "mairie.fr").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(
        detail.ecosystem.state_of(EcosystemModule::LegalLife).status,
        ModuleStatus::Pending
    );
    assert_eq!(
        detail
            .ecosystem
            .state_of(EcosystemModule::PublicContracts)
            .status,
        ModuleStatus::Done
    );
    assert_eq!(detail.evaluation.stage, EvaluationStage::NotEvaluated);
    assert_eq!(
        (
            detail.evaluation.modules_done,
            detail.evaluation.modules_available
        ),
        (6, 6)
    );
    let readiness = harness.usecase.ecosystem_readiness().await.unwrap();
    assert!(!readiness.is_ready(EcosystemModule::LegalLife));
    assert!(readiness.is_ready(EcosystemModule::OfficialIdentity));
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(
        harness.listed_evaluation(profile, &lead).await.stage,
        EvaluationStage::InspectionComplete
    );

    harness.provider(BODACC, "ready", true).await;
    harness.inspect(&EcosystemModule::AVAILABLE).await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(
        detail.ecosystem.state_of(EcosystemModule::LegalLife).status,
        ModuleStatus::Done
    );
    assert!(
        detail
            .ecosystem
            .findings_of(EcosystemModule::LegalLife)
            .any(|found| found.value == "FACT-OF-legal_life")
    );
    assert_eq!(
        detail.evaluation.stage,
        EvaluationStage::RequalificationPlanned
    );
    assert_eq!(harness.qualify().await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_page_lists_every_module_and_the_progress_follows_the_inspection(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    assert_eq!(harness.qualify().await, 1);

    harness.inspect(&[EcosystemModule::WebsiteStack]).await;
    let lead = harness.lead(profile, "mairie.fr").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    let statuses: Vec<(EcosystemModule, ModuleStatus)> = detail
        .enrichment
        .modules
        .iter()
        .map(|state| (state.module, state.status))
        .collect();
    assert_eq!(
        statuses,
        [
            (EcosystemModule::OfficialIdentity, ModuleStatus::Pending),
            (EcosystemModule::LegalLife, ModuleStatus::Pending),
            (EcosystemModule::PublicContracts, ModuleStatus::Pending),
            (EcosystemModule::WebsiteStack, ModuleStatus::Done),
            (EcosystemModule::DnsEmail, ModuleStatus::Pending),
            (EcosystemModule::HiringActivity, ModuleStatus::Pending),
            (EcosystemModule::RecentActivity, ModuleStatus::Pending)
        ]
    );
    assert_eq!(harness.qualify().await, 1);
    let running = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(running.stage, EvaluationStage::InspectionRunning);
    assert_eq!((running.modules_done, running.modules_available), (1, 7));

    harness
        .inspect(&[
            EcosystemModule::OfficialIdentity,
            EcosystemModule::LegalLife,
            EcosystemModule::PublicContracts,
            EcosystemModule::DnsEmail,
            EcosystemModule::HiringActivity,
            EcosystemModule::RecentActivity,
        ])
        .await;
    assert_eq!(harness.qualify().await, 1);
    let complete = harness.listed_evaluation(profile, &lead).await;
    assert_eq!(complete.stage, EvaluationStage::InspectionComplete);
    assert_eq!((complete.modules_done, complete.modules_available), (7, 7));
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.evaluation, complete);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_certificate_only_lead_waits_for_evidence_then_qualifies_on_a_new_signal(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind(
            "acme.com",
            SourceName::CertificateTransparency,
            "certificate_seen",
            10,
        )
        .await;
    harness.created_at_clock_zero().await;
    harness.inspect(&EcosystemModule::AVAILABLE).await;
    harness
        .observe_kind("acme.com", SourceName::RoleGuess, "role_contact_found", 40)
        .await;

    harness.set_time(5_000);
    assert_eq!(harness.qualify().await, 0);
    assert!(harness.calls(false).is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.evaluation.stage, EvaluationStage::CertificateOnly);
    assert_eq!(
        harness.listed_evaluation(profile, &lead).await,
        detail.evaluation
    );

    harness
        .observe_kind("acme.com", SourceName::HttpProbe, "product_detected", 50)
        .await;
    assert_eq!(harness.qualify().await, 1);
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.evaluation.stage, EvaluationStage::InspectionComplete);
    assert_eq!(
        harness.listed_evaluation(profile, &lead).await.stage,
        EvaluationStage::InspectionComplete
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn certificates_hosts_follow_the_country_filter_of_their_agent(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;

    let foreign = harness.record_host("acme.de", "certificats").await;
    assert_eq!(foreign.out_of_country, 1);
    let listed = harness.record_host("acme.fr", "certificats").await;
    assert_eq!(listed, Skipped::default());
    let generic = harness.record_host("globex.com", "certificats").await;
    assert_eq!(generic, Skipped::default());
    let tenant = harness.record_host("acme.okta.com", "certificats").await;
    assert_eq!(tenant.saas_tenant, 1);
    let leads: Vec<String> =
        query_scalar("SELECT domain FROM leads WHERE profile_id = $1 ORDER BY domain")
            .bind(profile.0)
            .fetch_all(&harness.pool)
            .await
            .unwrap();
    assert_eq!(leads, ["acme.fr", "globex.com"]);

    let mut abroad = complete_build();
    if let Some(official) = abroad.official.as_mut() {
        official.head_office_country_code = Some("US".to_owned());
    }
    let report = harness
        .research(&FakeCompany::new(vec![abroad.clone(), abroad]))
        .await;
    assert_eq!(report.excluded_out_of_country, 1);
    let globex = harness.lead(profile, "globex.com").await;
    assert!(harness.usecase.lead_detail(globex.id).await.is_err());
    let reason: Option<String> = query_scalar("SELECT excluded_reason FROM leads WHERE id = $1")
        .bind(globex.id.0)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(reason.as_deref(), Some("hors pays visé"));
    let fr = harness.lead(profile, "acme.fr").await;
    assert!(harness.usecase.lead_detail(fr.id).await.is_ok());
    let listing = harness
        .usecase
        .list_leads(
            profile,
            LeadQuery {
                limit: 50,
                ..LeadQuery::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(listing.page.total, 1);

    let after = harness.record_host("initech.com", "certificats").await;
    assert_eq!(after, Skipped::default());
    harness
        .usecase
        .request_company_refresh_at(harness.lead(profile, "initech.com").await.id, at(0))
        .await
        .unwrap();
    let mut initech = complete_build();
    if let Some(official) = initech.official.as_mut() {
        official.head_office_country_code = Some("DE".to_owned());
    }
    harness.research(&FakeCompany::new(vec![initech])).await;
    let again = harness.record_host("initech.com", "certificats").await;
    assert_eq!(again.out_of_country, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_and_never_inspected_domains_are_inspected_before_the_refreshes(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    let visited = Arc::new(Mutex::new(Vec::new()));
    let runner = RecordingModule(Arc::clone(&visited));
    harness.observe("old.fr", SourceName::HttpProbe, 10).await;
    harness.created_ago("old.fr", 3 * 86_400).await;
    harness.inspect_batches(&runner, 30, 10).await;
    harness.modules_due_again().await;
    visited.lock().unwrap().clear();

    for host in ["fresh.fr", "backlog.fr"] {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    harness.created_ago("fresh.fr", 60).await;
    harness.created_ago("backlog.fr", 2 * 86_400).await;
    harness.inspect_batches(&runner, 1, 1).await;
    assert_eq!(*visited.lock().unwrap(), ["fresh.fr", "old.fr"]);

    visited.lock().unwrap().clear();
    harness.inspect_batches(&runner, 30, 10).await;
    assert_eq!(*visited.lock().unwrap(), ["backlog.fr"]);

    visited.lock().unwrap().clear();
    harness.modules_due_again().await;
    harness.created_ago("old.fr", 30).await;
    harness.inspect_batches(&runner, 30, 10).await;
    assert_eq!(
        *visited.lock().unwrap(),
        ["old.fr", "fresh.fr", "backlog.fr"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_manual_reevaluation_calls_the_models_again_and_refreshes_the_cache(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    let lead = harness.lead(profile, "acme.com").await;
    let first = (
        harness.calls(false).len(),
        harness.scoring_calls(),
        harness.calls(true).len(),
    );
    assert_eq!(first, (1, 1, 1));

    harness.set_time(10);
    query(
        "INSERT INTO lead_requalifications (lead_id, requalify_after, marked_at, forced) VALUES ($1, $2, $2, TRUE)",
    )
    .bind(lead.id.0)
    .bind(harness.clock.now())
    .execute(&harness.pool)
    .await
    .unwrap();
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    assert_eq!(
        (
            harness.calls(false).len(),
            harness.scoring_calls(),
            harness.calls(true).len()
        ),
        first
    );
    let cached_before: DateTime<Utc> = query_scalar("SELECT MAX(created_at) FROM llm_cache")
        .fetch_one(&harness.pool)
        .await
        .unwrap();

    harness.set_time(20);
    harness
        .usecase
        .request_lead_reevaluation_at(lead.id, harness.clock.now())
        .await
        .unwrap();
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    assert_eq!(
        (
            harness.calls(false).len(),
            harness.scoring_calls(),
            harness.calls(true).len()
        ),
        (2, 2, 2)
    );
    let cached_after: DateTime<Utc> = query_scalar("SELECT MIN(created_at) FROM llm_cache")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert!(cached_after > cached_before);
    let fresh: bool =
        query_scalar("SELECT fresh_calls FROM lead_requalifications WHERE lead_id = $1")
            .bind(lead.id.0)
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert!(!fresh);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_evaluated_before_their_latest_dossier_are_marked_once_at_startup(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.observe("other.fr", SourceName::HttpProbe, 10).await;
    harness.research(&FakeCompany::new(Vec::new())).await;
    assert_eq!(harness.qualify().await, 2);
    assert_eq!(harness.qualify().await, 0);
    query("UPDATE company_dossiers SET content_at = $1 WHERE domain = 'acme.com'")
        .bind(at(1_500))
        .execute(&harness.pool)
        .await
        .unwrap();
    harness.set_time(600);

    let marked = harness
        .usecase
        .backfill_dossier_requalifications(harness.clock.now())
        .await
        .unwrap();
    assert_eq!(marked, 1);
    assert_eq!(
        harness
            .usecase
            .backfill_dossier_requalifications(harness.clock.now())
            .await
            .unwrap(),
        0
    );
    let acme = harness.lead(profile, "acme.com").await;
    assert_eq!(
        harness.listed_evaluation(profile, &acme).await.stage,
        EvaluationStage::RequalificationPlanned
    );
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.qualify().await, 0);
    assert_eq!(
        harness
            .usecase
            .backfill_dossier_requalifications(harness.clock.now())
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_allowed_sourcing_signal_lifts_the_out_of_country_exclusion_only(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    harness.record_host("globex.com", "certificats").await;
    harness.observe("okta.com", SourceName::HttpProbe, 10).await;
    harness
        .usecase
        .exclude_saas_tenant_leads(harness.clock.now())
        .await
        .unwrap();
    let mut abroad = complete_build();
    if let Some(official) = abroad.official.as_mut() {
        official.head_office_country_code = Some("US".to_owned());
    }
    harness
        .research(&FakeCompany::new(vec![abroad.clone(), abroad]))
        .await;
    assert_eq!(
        harness.exclusion(profile, "globex.com").await.as_deref(),
        Some("hors pays visé")
    );

    let filtered = harness.record_host("globex.com", "certificats").await;
    assert_eq!(filtered.out_of_country, 1);
    assert_eq!(
        harness.exclusion(profile, "globex.com").await.as_deref(),
        Some("hors pays visé")
    );

    harness.record_search("globex.com", "marches-publics").await;
    assert_eq!(harness.exclusion(profile, "globex.com").await, None);
    let globex = harness.lead(profile, "globex.com").await;
    assert!(harness.usecase.lead_detail(globex.id).await.is_ok());

    harness.record_search("okta.com", "marches-publics").await;
    assert_eq!(
        harness.exclusion(profile, "okta.com").await.as_deref(),
        Some("domaine d'un fournisseur SaaS")
    );

    harness.research(&FakeCompany::new(Vec::new())).await;
    harness.set_time(91 * 86_400);
    let mut again = complete_build();
    if let Some(official) = again.official.as_mut() {
        official.head_office_country_code = Some("US".to_owned());
    }
    harness.research(&FakeCompany::new(vec![again])).await;
    assert_eq!(harness.exclusion(profile, "globex.com").await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_on_saas_tenant_domains_are_suppressed_once(pool: PgPool) {
    let harness = Harness::new(pool);
    let profile = harness.profile().await;
    for host in ["acme.okta.com", "acme.fr", "salesforce.com"] {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }

    let first = harness
        .usecase
        .exclude_saas_tenant_leads(harness.clock.now())
        .await
        .unwrap();
    assert_eq!(first, 2);
    assert_eq!(
        harness
            .usecase
            .exclude_saas_tenant_leads(harness.clock.now())
            .await
            .unwrap(),
        0
    );
    let reasons: Vec<(String, Option<String>)> =
        query_as("SELECT domain, excluded_reason FROM leads WHERE profile_id = $1 ORDER BY domain")
            .bind(profile.0)
            .fetch_all(&harness.pool)
            .await
            .unwrap();
    assert_eq!(
        reasons,
        [
            ("acme.fr".to_owned(), None),
            (
                "okta.com".to_owned(),
                Some("domaine d'un fournisseur SaaS".to_owned())
            ),
            (
                "salesforce.com".to_owned(),
                Some("domaine d'un fournisseur SaaS".to_owned())
            ),
        ]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_build_reports_its_unconfirmed_links_and_stores_their_count(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    let mut build = complete_build();
    build.unconfirmed.record(LinkRejection::Soft404Baseline);
    build.unconfirmed.record(LinkRejection::Status);
    let report = harness.research(&FakeCompany::new(vec![build])).await;
    assert_eq!(report.unconfirmed_links.total(), 2);
    assert_eq!(report.unconfirmed_links.soft_404_baseline, 1);
    let dossier = harness
        .usecase
        .company_dossier(&domain("acme.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dossier.ignored_links, 2);
    assert!(dossier.pages_checked_at.is_some());
    assert_eq!(
        harness
            .usecase
            .count_unchecked_company_pages()
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_page_recheck_removes_unconfirmed_pages_once_and_requalifies_the_lead(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.research(&FakeCompany::new(Vec::new())).await;
    assert_eq!(harness.qualify().await, 1);
    harness.forget_page_checks().await;
    assert_eq!(
        harness
            .usecase
            .count_unchecked_company_pages()
            .await
            .unwrap(),
        1
    );

    harness.set_time(60);
    let verifier = FakeVerifier::rejecting(&[(PAGE_URL, LinkRejection::Soft404Baseline)]);
    let report = harness.recheck(&verifier).await;

    assert_eq!(report.dossiers, 1);
    assert_eq!(report.changed, 1);
    assert_eq!(report.unconfirmed_links.soft_404_baseline, 1);
    assert_eq!(report.remaining, 0);
    assert_eq!(report.retry_after, None);
    let dossier = harness
        .usecase
        .company_dossier(&domain("acme.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        dossier
            .pages
            .iter()
            .map(|page| page.url.as_str())
            .collect::<Vec<_>>(),
        ["https://acme.com/"]
    );
    assert_eq!(dossier.ignored_links, 1);
    assert_eq!(dossier.content_at, Some(at(1_060)));
    assert_eq!(harness.qualify().await, 1);

    let again = harness.recheck(&verifier).await;
    assert_eq!(again.dossiers, 0);
    assert_eq!(verifier.calls(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_recheck_that_confirms_every_page_does_not_requalify(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.research(&FakeCompany::new(Vec::new())).await;
    assert_eq!(harness.qualify().await, 1);
    harness.forget_page_checks().await;

    harness.set_time(60);
    let report = harness.recheck(&FakeVerifier::rejecting(&[])).await;

    assert_eq!(report.dossiers, 1);
    assert_eq!(report.changed, 0);
    assert_eq!(report.unconfirmed_links.total(), 0);
    assert_eq!(harness.qualify().await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rate_limited_recheck_keeps_the_pages_and_asks_to_retry(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.research(&FakeCompany::new(Vec::new())).await;
    harness.forget_page_checks().await;
    let mut verifier = FakeVerifier::rejecting(&[(PAGE_URL, LinkRejection::Status)]);
    verifier.rate_limited = true;

    let report = harness.recheck(&verifier).await;

    assert_eq!(report.dossiers, 0);
    assert_eq!(report.retry_after, Some(Duration::from_secs(30)));
    assert_eq!(report.remaining, 1);
    let dossier = harness
        .usecase
        .company_dossier(&domain("acme.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dossier.pages.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_agent_document_set_carries_the_ecosystem_findings(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    harness.inspect(&EcosystemModule::AVAILABLE).await;

    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.summarise().await, 1);
    let calls = harness.every_call();
    assert!(calls.iter().any(|call| call.scoring));
    assert!(calls.iter().any(|call| call.aggregator));
    for call in &calls {
        let ecosystem: Vec<&(String, String)> = call
            .documents
            .iter()
            .filter(|(_, content)| content.starts_with("Ecosystem, module"))
            .collect();
        assert_eq!(ecosystem.len(), 7, "{:?}", call.documents);
        let website = ecosystem
            .iter()
            .find(|(source, _)| source == "https://mairie.fr/website_stack")
            .unwrap();
        assert!(website.1.contains("- Fait observé: FACT-OF-website_stack"));
        let legal = ecosystem
            .iter()
            .find(|(source, _)| source == "https://mairie.fr/legal_life")
            .unwrap();
        assert!(legal.1.contains(
            "- Fait observé (2026-09-10): FACT-OF-legal_life; court Greffe du tribunal de commerce de Lyon"
        ));
        let contracts = ecosystem
            .iter()
            .find(|(source, _)| source == "https://mairie.fr/public_contracts")
            .unwrap();
        assert!(contracts.1.contains(
            "- Fait observé (2026-08-10): FACT-OF-public_contracts; buyer Commune de Lorient; amount 1200.50 EUR; procedure Procédure adaptée"
        ));
    }
    let aggregator = calls.iter().find(|call| call.aggregator).unwrap();
    assert!(
        aggregator
            .instruction
            .contains("are the company dossier and the ecosystem findings")
    );
    let qualification = calls
        .iter()
        .find(|call| !call.aggregator && !call.scoring)
        .unwrap();
    assert!(qualification.system.contains("start with \"Ecosystem\""));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_domain_on_a_hosting_platform_gets_no_dossier_read(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe("jawagg.notion.site", SourceName::HttpProbe, 10)
        .await;
    harness.created_at_clock_zero().await;
    let builder = FakeCompany::new(Vec::new());

    assert_eq!(harness.qualify().await, 1);
    let report = harness.research(&builder).await;

    assert_eq!(report.domains, 1);
    assert!(builder.calls().is_empty());
    let lead = harness.lead(profile, "jawagg.notion.site").await;
    let dossier = harness
        .usecase
        .company_dossier(&lead.domain)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dossier.state, DossierState::HostingPlatform);
    assert!(dossier.pages.is_empty());
    assert!(dossier.official.is_none());
    assert!(dossier.built_at.is_some());
    assert_eq!(harness.research(&builder).await.domains, 0);
    assert_eq!(harness.qualify().await, 0);
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.company.unwrap().state, DossierState::HostingPlatform);
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_dossiers_of_platform_domains_are_settled_once(pool: PgPool) {
    let harness = Harness::new(pool);
    for domain in ["jawagg.notion.site", "acme.com"] {
        query(
            "INSERT INTO company_dossiers (domain, state, pages, built_at, next_attempt_at)
             VALUES ($1, 'partial', '[{\"url\": \"https://x/mentions-legales\", \"text\": \"guessed\"}]'::jsonb, now(), now() + interval '90 days')",
        )
        .bind(domain)
        .execute(&harness.pool)
        .await
        .unwrap();
    }

    let settled = harness
        .usecase
        .settle_platform_dossiers(NINETY_DAYS, &harness.clock)
        .await
        .unwrap();

    assert_eq!(settled, 1);
    let platform = harness
        .usecase
        .company_dossier(&domain("jawagg.notion.site"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(platform.state, DossierState::HostingPlatform);
    assert!(platform.pages.is_empty());
    let other = harness
        .usecase
        .company_dossier(&domain("acme.com"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.state, DossierState::Partial);
    assert_eq!(other.pages.len(), 1);
    assert_eq!(
        harness
            .usecase
            .settle_platform_dossiers(NINETY_DAYS, &harness.clock)
            .await
            .unwrap(),
        0
    );
}

fn paced(
    modules: &[EcosystemModule],
    visits: &Arc<Visits>,
    limited: Option<EcosystemModule>,
) -> Vec<PacedModule> {
    modules
        .iter()
        .map(|module| PacedModule {
            module: *module,
            visits: Arc::clone(visits),
            limited: limited == Some(*module),
        })
        .collect()
}

const INSPECTED: [&str; 3] = ["acme.com", "globex.com", "initech.com"];

#[sqlx::test(migrations = "../../migrations")]
async fn modules_of_a_domain_and_several_domains_are_inspected_at_once(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    for host in INSPECTED {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    let visits = Arc::new(Visits::default());
    let modules = [
        EcosystemModule::OfficialIdentity,
        EcosystemModule::LegalLife,
        EcosystemModule::WebsiteStack,
        EcosystemModule::DnsEmail,
    ];
    let runners = paced(&modules, &visits, None);

    let report = harness.inspect_at_once(&runners, 2).await;

    assert_eq!(
        (report.domains, report.findings, report.failed),
        (12, 12, 0)
    );
    let (domains_at_once, modules_at_once) = *visits.peaks.lock().unwrap();
    assert_eq!(domains_at_once, 2);
    assert!(
        modules_at_once > 1,
        "the modules of a domain never overlapped"
    );
    for host in INSPECTED {
        let (_, identified) = visits.span(EcosystemModule::OfficialIdentity, host);
        let (legal, _) = visits.span(EcosystemModule::LegalLife, host);
        assert!(
            identified <= legal,
            "{host}: legal life read before the identity"
        );
    }
    let stats = harness
        .usecase
        .ecosystem_stats(harness.clock.now())
        .await
        .unwrap();
    for module in modules {
        let stat = stats.iter().find(|stat| stat.module == module).unwrap();
        assert_eq!(
            (stat.leads_inspected, stat.findings_count),
            (3, 3),
            "{module:?}"
        );
        assert!(stat.latest_example.is_some());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rate_limited_module_pauses_alone_for_the_rest_of_the_pass(pool: PgPool) {
    let harness = Harness::new(pool);
    harness.profile().await;
    for host in INSPECTED {
        harness.observe(host, SourceName::HttpProbe, 10).await;
    }
    let visits = Arc::new(Visits::default());
    let runners = paced(
        &[EcosystemModule::WebsiteStack, EcosystemModule::DnsEmail],
        &visits,
        Some(EcosystemModule::DnsEmail),
    );

    let report = harness.inspect_at_once(&runners, 1).await;

    assert!(report.rate_limited);
    assert_eq!(report.domains, 3);
    assert_eq!(visits.spans.lock().unwrap().len(), 3);
    let stats = harness
        .usecase
        .ecosystem_stats(harness.clock.now())
        .await
        .unwrap();
    let dns = stats
        .iter()
        .find(|stat| stat.module == EcosystemModule::DnsEmail)
        .unwrap();
    assert_eq!(dns.leads_inspected, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn inspection_modules_and_requalification_marks_are_on_the_timeline(pool: PgPool) {
    let harness = Harness::new(pool).requalifying(DEBOUNCE, 3);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    assert_eq!(harness.qualify().await, 1);
    let lead = harness.lead(profile, "mairie.fr").await;

    harness.set_time(10);
    harness.inspect(&[EcosystemModule::WebsiteStack]).await;

    let timeline = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let numbers: Vec<u32> = timeline.rounds.iter().map(|round| round.number).collect();
    assert_eq!(numbers, vec![2, 1]);
    let next: Vec<&TimelineEvent> = timeline.rounds[0].events.iter().collect();
    let module = next
        .iter()
        .find(|event| event.kind == timeline_kind::MODULE)
        .unwrap();
    assert_eq!(module.step, TimelineStep::Inspection);
    assert_eq!(module.status, EventStatus::Succeeded);
    assert_eq!(module.actor.module.as_deref(), Some("website_stack"));
    assert_eq!(module.detail["changed"], true);
    assert!(next.iter().any(|event| {
        event.step == TimelineStep::Requalification && event.kind == timeline_kind::PLANNED
    }));
    assert_eq!(timeline.rounds[0].trigger, RoundTrigger::Pending);
    let qualification = timeline
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Qualification)
        .unwrap();
    assert_eq!(qualification.status, StageStatus::Waiting);
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::RequalificationPlanned)
    );
    assert_eq!(qualification.next_at, Some(at(1_000 + 10 + 120)));

    harness.set_time(200);
    assert_eq!(harness.qualify().await, 1);
    let requalified = harness.usecase.lead_timeline(lead.id).await.unwrap();
    assert_eq!(requalified.rounds[0].number, 2);
    assert_eq!(requalified.rounds[0].trigger, RoundTrigger::NewInformation);
    assert_eq!(
        requalified.rounds[0].sources,
        vec!["website_stack".to_owned()]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_capped_requalification_and_a_manual_reevaluation_show_in_the_pipeline(pool: PgPool) {
    let harness = Harness::new(pool).requalifying(Duration::ZERO, 1);
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;
    assert_eq!(harness.qualify().await, 1);
    let lead = harness.lead(profile, "mairie.fr").await;
    let round = Arc::new(AtomicI64::new(1));
    let runners = [ChangingModule(
        EcosystemModule::WebsiteStack,
        Arc::clone(&round),
    )];
    harness.set_time(100);
    harness.modules_due_again().await;
    harness.inspect_with(&runners).await;
    assert_eq!(harness.qualify().await, 1);
    round.store(2, Ordering::SeqCst);
    harness.set_time(200);
    harness.modules_due_again().await;
    harness.inspect_with(&runners).await;
    assert_eq!(harness.qualify().await, 0);

    let capped = harness.usecase.lead_timeline(lead.id).await.unwrap();
    let qualification = capped
        .pipeline
        .stages
        .iter()
        .find(|state| state.stage == Stage::Qualification)
        .unwrap();
    assert_eq!(
        qualification.reason_code,
        Some(StageReason::RequalificationCapped)
    );
    assert_eq!(qualification.next_at, Some(at(1_000 + NEXT_UTC_DAY)));

    harness.set_time(210);
    harness
        .usecase
        .request_lead_reevaluation_at(lead.id, harness.clock.now())
        .await
        .unwrap();
    assert_eq!(harness.qualify().await, 1);
    let manual = harness.usecase.lead_timeline(lead.id).await.unwrap();
    assert_eq!(manual.rounds[0].number, 3);
    assert_eq!(manual.rounds[0].trigger, RoundTrigger::Manual);
}
