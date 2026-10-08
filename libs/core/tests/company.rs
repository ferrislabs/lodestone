mod support;

use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, NaiveDate, Utc};
use common::{EncryptionKey, Secret};
use lodestone_core::{
    ActiveProfile, AtsTargets, CandidateHosts, Clock, CompanyBuilder, CompanyPage,
    CreateProfileCommand, DossierBuild, DossierState, EnrichmentStatus, EnrichmentStep,
    GenerationParams, Hostname, Lead, LeadRepository, LlmError, LlmProvider, LlmRequest,
    LlmResponse, LodestoneUseCase, NormalizedDomain, Observation, OfficialData, ProfileId,
    ProfileName, Provenance, ProviderDefinition, ProviderKind, SignalKind, Siren, SourceError,
    SourceName, Tier, TokenPricing, TokenUsage, UnitInterval, ValidatedProfile, WaitingReason,
    company::{CompanyRunReport, CompanyRunSettings},
    evaluation::{EvaluationSettings, ProviderBuilder},
    infrastructure::{
        candidate_host::PgCandidateHosts, contact_target::postgres::PgAtsTargets,
        crypto::AesGcmCipher, lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache,
        postgres::with_tx,
    },
    llm::CreateLlmProvider,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const MODEL: &str = "model-a";
const PAGE_URL: &str = "https://acme.com/a-propos";
const PAGE_TEXT: &str = "Nous fabriquons des pieces mecaniques depuis 2010";
const WAIT: Duration = Duration::from_secs(600);
const ENRICHMENT_WAIT: Duration = Duration::from_secs(1_800);
const NINETY_DAYS: Duration = Duration::from_secs(90 * 86_400);

#[derive(Clone)]
struct TestClock(Arc<AtomicI64>);

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
    }
}

fn unreachable_build() -> DossierBuild {
    DossierBuild {
        siren: None,
        official: None,
        pages: Vec::new(),
        reachable: false,
        failure: Some("robots.txt disallows the home page".to_owned()),
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

    fn waiting(mut self) -> Self {
        self.usecase = self.usecase.with_company_wait(WAIT);
        self
    }

    fn enriching(mut self) -> Self {
        self.usecase = self.usecase.with_enrichment_wait(ENRICHMENT_WAIT);
        self
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

    async fn probe_every_lead_host(&self, profile: ProfileId, seconds: i64) {
        let hosts = PgCandidateHosts::new(self.pool.clone());
        hosts
            .seed_lead_hosts(profile, &["sso".to_owned()], at(seconds), 50)
            .await
            .unwrap();
        let due: Vec<Hostname> = hosts
            .list_due(SourceName::HttpProbe, at(seconds), 50)
            .await
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.host)
            .collect();
        hosts
            .mark_checked(&due, SourceName::HttpProbe, at(seconds))
            .await
            .unwrap();
    }

    async fn read_company_jobs(&self, host: &str, seconds: i64) {
        PgAtsTargets::new(self.pool.clone())
            .mark_checked(&[domain(host)], at(seconds))
            .await
            .unwrap();
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
async fn the_agents_wait_for_the_dossier_then_run_without_it_after_the_wait(pool: PgPool) {
    let harness = Harness::new(pool).waiting();
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.created_at_clock_zero().await;

    assert_eq!(harness.qualify().await, 0);
    harness.set_time(599);
    assert_eq!(harness.qualify().await, 0);
    assert!(harness.calls(false).is_empty());

    harness.set_time(601);
    assert_eq!(harness.qualify().await, 1);
    let calls = harness.calls(false);
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0]
            .documents
            .iter()
            .all(|(source, _)| !source.contains("acme.com/a-propos"))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_dossier_that_is_not_pending_releases_the_agents_before_the_wait_ends(pool: PgPool) {
    let harness = Harness::new(pool).waiting();
    harness.defaults().await;
    harness.profile().await;
    harness.observe("acme.com", SourceName::HttpProbe, 10).await;
    harness.created_at_clock_zero().await;
    let builder = FakeCompany::new(vec![unreachable_build()]);

    assert_eq!(harness.qualify().await, 0);
    harness.research(&builder).await;
    assert_eq!(harness.qualify().await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_dossier_makes_the_agents_due_again_and_the_prompts_carry_it(pool: PgPool) {
    let harness = Harness::new(pool).waiting();
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
async fn the_agents_wait_for_the_enrichment_then_run_after_the_timeout(pool: PgPool) {
    let harness = Harness::new(pool).enriching();
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;

    assert_eq!(harness.qualify().await, 0);
    let lead = harness.lead(profile, "mairie.fr").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.enrichment.status, EnrichmentStatus::Pending);
    assert_eq!(
        detail.enrichment.waiting_reason,
        Some(WaitingReason::AwaitingEnrichment)
    );
    harness.set_time(1_799);
    assert_eq!(harness.qualify().await, 0);
    assert!(harness.calls(false).is_empty());

    harness.set_time(1_801);
    assert_eq!(harness.qualify().await, 1);
    assert_eq!(harness.calls(false).len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_agents_run_once_every_enabled_enrichment_step_ran(pool: PgPool) {
    let harness = Harness::new(pool).enriching();
    harness.defaults().await;
    let profile = harness.profile().await;
    harness
        .observe_kind("mairie.fr", SourceName::Boamp, "public_tender", 10)
        .await;
    harness.created_at_clock_zero().await;

    harness.probe_every_lead_host(profile, 20).await;
    assert_eq!(harness.qualify().await, 0);
    let lead = harness.lead(profile, "mairie.fr").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    let ran: Vec<(EnrichmentStep, bool)> = detail
        .enrichment
        .steps
        .iter()
        .map(|step| (step.step, step.ran_at.is_some()))
        .collect();
    assert_eq!(
        ran,
        [
            (EnrichmentStep::HttpProbe, true),
            (EnrichmentStep::CompanyJobs, false)
        ]
    );

    harness.read_company_jobs("mairie.fr", 30).await;
    assert_eq!(harness.qualify().await, 1);
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.enrichment.status, EnrichmentStatus::Complete);
    assert_eq!(detail.enrichment.waiting_reason, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_certificate_only_lead_waits_for_evidence_then_qualifies_on_a_new_signal(pool: PgPool) {
    let harness = Harness::new(pool).enriching();
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
    harness.probe_every_lead_host(profile, 20).await;
    harness.read_company_jobs("acme.com", 30).await;
    harness
        .observe_kind("acme.com", SourceName::RoleGuess, "role_contact_found", 40)
        .await;

    harness.set_time(5_000);
    assert_eq!(harness.qualify().await, 0);
    assert!(harness.calls(false).is_empty());
    let lead = harness.lead(profile, "acme.com").await;
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.enrichment.status, EnrichmentStatus::Complete);
    assert_eq!(
        detail.enrichment.waiting_reason,
        Some(WaitingReason::AwaitingEnrichmentEvidence)
    );

    harness
        .observe_kind("acme.com", SourceName::HttpProbe, "product_detected", 50)
        .await;
    assert_eq!(harness.qualify().await, 1);
    let detail = harness.usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.enrichment.waiting_reason, None);
}
