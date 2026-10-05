mod support;

use std::{
    num::NonZeroU32,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Candidate, Clock, ContactMethod, ContactRepository, CreateProfileCommand,
    EmailAddress, LeadRepository, LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase,
    NormalizedDomain, ProfileId, ProfileName, Provenance, RunRepository, RunStatus, SignalKind,
    SignalRepository, SourceError, SourceName, SuppressionTarget, TokenPricing, TokenUsage,
    ValidatedProfile,
    definition::{KeywordSource, ProfileDefinition},
    infrastructure::{
        contact::postgres::PgContactRepository, lead::postgres::PgLeadRepository,
        llm_cache::postgres::PgLlmCache, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
    llm::gateway::CallSettings,
    triage::{CandidateSource, TriageSettings},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct FakeSource {
    candidates: Vec<Candidate>,
}

impl CandidateSource for FakeSource {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    fn rules<'a>(&self, profile: &'a ProfileDefinition) -> Option<&'a KeywordSource> {
        profile.boamp()
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

struct ScriptedProvider {
    verdicts: Vec<(&'static str, Value)>,
    input_tokens: u64,
    calls: AtomicU64,
}

impl ScriptedProvider {
    fn new(verdicts: Vec<(&'static str, Value)>) -> Self {
        Self {
            verdicts,
            input_tokens: 10,
            calls: AtomicU64::new(0),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl LlmProvider for ScriptedProvider {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let content = request.documents.first().map_or("", |d| d.content);
        let output = self
            .verdicts
            .iter()
            .find(|(marker, _)| content.contains(marker))
            .map_or_else(irrelevant, |(_, verdict)| verdict.clone());
        Ok(LlmResponse {
            output,
            model: "scripted".to_owned(),
            usage: TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: 1,
            },
        })
    }
}

fn irrelevant() -> Value {
    json!({"relevant": false, "reason": "off topic", "evidence": [], "contact_email": null})
}

fn relevant(quote: &str, contact: Option<&str>) -> Value {
    json!({
        "relevant": true,
        "reason": "identity project",
        "evidence": [{"document_index": 0, "quote": quote}],
        "contact_email": contact,
    })
}

fn candidate(host: &str, title: &str) -> Candidate {
    Candidate {
        domain: domain(host),
        source: SourceName::Boamp,
        kind: SignalKind::parse("public_tender").unwrap(),
        title: title.to_owned(),
        text: "Avis de marche public".to_owned(),
        provenance: Provenance::parse(&format!("https://marches.example/avis/{host}")).unwrap(),
        observed_at: at(50),
        details: json!({"idweb": host}),
    }
}

fn settings(budget_cents: u32, cents_per_mtok: u32) -> TriageSettings {
    TriageSettings {
        call: CallSettings {
            max_retries: 0,
            pricing: TokenPricing {
                input_cents_per_mtok: cents_per_mtok,
                output_cents_per_mtok: 0,
            },
            base_delay: Duration::ZERO,
        },
        max_tokens: 512,
        budget_cents: NonZeroU32::new(budget_cents).unwrap(),
    }
}

async fn activate(pool: &PgPool, name: &str, definition: Value) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: ValidatedProfile::parse(definition).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn iam_profile(pool: &PgPool) -> ProfileId {
    activate(pool, "iam", serde_json::from_str(IAM).unwrap()).await
}

async fn run_triage(
    pool: &PgPool,
    source: &FakeSource,
    provider: &ScriptedProvider,
    settings: TriageSettings,
) -> lodestone_core::triage::TriageReport {
    run_triage_with(pool, source, Some(provider), settings).await
}

async fn run_triage_with(
    pool: &PgPool,
    source: &FakeSource,
    provider: Option<&ScriptedProvider>,
    settings: TriageSettings,
) -> lodestone_core::triage::TriageReport {
    LodestoneUseCase::new(pool.clone())
        .run_triage(
            source,
            provider,
            &PgLlmCache::new(pool.clone()),
            settings,
            &FixedClock,
        )
        .await
        .unwrap()
}

async fn signals(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(host).unwrap())
            .await?
        else {
            return Ok(Vec::new());
        };
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn contacts(
    pool: &PgPool,
    profile: ProfileId,
    host: &str,
) -> Vec<lodestone_core::ContactEmail> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(host).unwrap())
            .await?
        else {
            return Ok(Vec::new());
        };
        PgContactRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn runs(pool: &PgPool) -> Vec<lodestone_core::Run> {
    with_tx(pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_ambiguous_candidates_reach_the_llm(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("mairie.fr", "Mise en place d'un SSO avec MFA"),
            candidate("meubles.fr", "Fourniture de mobilier de bureau"),
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
        ],
    };
    let provider = ScriptedProvider::new(vec![("Badge", relevant("Badge d'acces et SSO", None))]);

    let report = run_triage(&pool, &source, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert_eq!(report.candidates, 4);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.sent_to_llm, 2);
    assert_eq!(report.accepted_by_llm, 1);
    assert!(!report.budget_exhausted);

    let accepted = signals(&pool, profile, "mairie.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].source, SourceName::Boamp);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(accepted[0].raw["details"]["idweb"], "mairie.fr");
    assert_eq!(accepted[0].strength.get(), 0.7);
    assert!(signals(&pool, profile, "meubles.fr").await.is_empty());
    let by_llm = signals(&pool, profile, "acme.com").await;
    assert_eq!(by_llm.len(), 1);
    assert_eq!(by_llm[0].raw["route"], "llm");
    assert_eq!(by_llm[0].raw["details"]["idweb"], "acme.com");
    assert_eq!(by_llm[0].raw["model"], "scripted");
    assert_eq!(by_llm[0].raw["prompt"], "triage@1");
    assert_eq!(by_llm[0].raw["from_cache"], false);
    assert!(signals(&pool, profile, "region.fr").await.is_empty());

    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
    assert_eq!(recorded[0].volume, 4);
    assert_eq!(recorded[0].signals, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn relevance_without_a_verbatim_quote_records_nothing(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
        ],
    };
    let provider = ScriptedProvider::new(vec![
        ("Badge", relevant("Badge d'acces et SSO", None)),
        (
            "IAM",
            relevant("a sentence the document never contains", None),
        ),
    ]);

    let report = run_triage(&pool, &source, &provider, settings(100, 0)).await;

    assert_eq!(report.sent_to_llm, 2);
    assert_eq!(report.accepted_by_llm, 1);
    assert_eq!(signals(&pool, profile, "acme.com").await.len(), 1);
    assert!(signals(&pool, profile, "region.fr").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn second_run_is_served_from_the_cache(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
        ],
    };
    let provider = ScriptedProvider::new(vec![("Badge", relevant("Badge d'acces et SSO", None))]);

    run_triage(&pool, &source, &provider, settings(100, 0)).await;
    assert_eq!(provider.calls(), 2);
    let second = run_triage(&pool, &source, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert_eq!(second.accepted_by_llm, 1);
    let kept = signals(&pool, profile, "acme.com").await;
    assert_eq!(kept.len(), 1);
    assert_eq!(runs(&pool).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppression_guards_the_llm_and_the_contacts(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .add_suppression(SuppressionTarget::Domain(domain("bad.fr")), None, None)
        .await
        .unwrap();
    usecase
        .add_suppression(
            SuppressionTarget::Email(EmailAddress::parse("dsi@corp.fr").unwrap()),
            None,
            None,
        )
        .await
        .unwrap();
    let source = FakeSource {
        candidates: vec![
            candidate("bad.fr", "Badge et SSO pour bad"),
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("corp.fr", "Annuaire corp et badge"),
        ],
    };
    let provider = ScriptedProvider::new(vec![
        (
            "Badge d'acces",
            relevant("Badge d'acces et SSO", Some("security@acme.com")),
        ),
        (
            "Annuaire corp",
            relevant("Annuaire corp et badge", Some("dsi@corp.fr")),
        ),
    ]);

    run_triage(&pool, &source, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert!(signals(&pool, profile, "bad.fr").await.is_empty());
    assert_eq!(signals(&pool, profile, "corp.fr").await.len(), 1);
    assert!(contacts(&pool, profile, "corp.fr").await.is_empty());
    let stored = contacts(&pool, profile, "acme.com").await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].address.as_str(), "security@acme.com");
    assert_eq!(stored[0].method, ContactMethod::LlmExtraction);
}

#[sqlx::test(migrations = "../../migrations")]
async fn exhausted_budget_stops_llm_calls_and_fails_the_run(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("mairie.fr", "Mise en place d'un SSO avec MFA"),
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
            candidate("ville.fr", "Projet SSO pour la ville"),
        ],
    };
    let mut provider =
        ScriptedProvider::new(vec![("Badge", relevant("Badge d'acces et SSO", None))]);
    provider.input_tokens = 1_000_000;

    let report = run_triage(&pool, &source, &provider, settings(1, 1)).await;

    assert_eq!(provider.calls(), 1);
    assert!(report.budget_exhausted);
    assert_eq!(report.sent_to_llm, 1);
    assert_eq!(signals(&pool, profile, "mairie.fr").await.len(), 1);
    assert_eq!(signals(&pool, profile, "acme.com").await.len(), 1);
    assert!(signals(&pool, profile, "region.fr").await.is_empty());
    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Failed);
    assert_eq!(recorded[0].error.as_deref(), Some("llm budget exhausted"));
    assert_eq!(recorded[0].signals, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_route_the_same_candidate_with_their_own_rules(pool: PgPool) {
    let strict = iam_profile(&pool).await;
    let mut lenient_definition: Value = serde_json::from_str(IAM).unwrap();
    let boamp = lenient_definition["agents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|agent| agent["source"]["kind"] == "boamp")
        .unwrap();
    boamp["source"] = json!({"kind": "boamp", "keywords": ["SSO"], "accept_min_keywords": 1});
    let lenient = activate(&pool, "lenient", lenient_definition).await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };
    let provider = ScriptedProvider::new(Vec::new());

    let report = run_triage(&pool, &source, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 1);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.sent_to_llm, 1);
    assert!(signals(&pool, strict, "ville.fr").await.is_empty());
    let accepted = signals(&pool, lenient, "ville.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(runs(&pool).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_a_provider_ambiguous_candidates_are_deferred(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("mairie.fr", "Mise en place d'un SSO avec MFA"),
            candidate("meubles.fr", "Fourniture de mobilier de bureau"),
            candidate("acme.com", "Badge d'acces et SSO"),
        ],
    };

    let report = run_triage_with(&pool, &source, None, settings(100, 0)).await;

    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.deferred, 1);
    assert_eq!(report.sent_to_llm, 0);
    assert_eq!(signals(&pool, profile, "mairie.fr").await.len(), 1);
    assert!(signals(&pool, profile, "acme.com").await.is_empty());
    let recorded = runs(&pool).await;
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
    assert_eq!(recorded[0].signals, 1);
}
