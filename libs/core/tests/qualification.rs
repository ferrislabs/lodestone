mod support;

use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Utc};
use lodestone_core::{
    Assessment, AssessmentRepository, Clock, CreateProfileCommand, LeadRepository, LeadStatus,
    LlmError, LlmProvider, LlmRequest, LlmResponse, LodestoneUseCase, ProfileId, ProfileName,
    Provenance, ReviseProfileCommand, RevisionNumber, RunRepository, RunStatus, Signal, SignalId,
    SignalKind, SignalRepository, SourceName, SuppressionTarget, TokenPricing, TokenUsage,
    UnitInterval, ValidatedProfile,
    infrastructure::{
        assessment::postgres::PgAssessmentRepository, lead::postgres::PgLeadRepository,
        llm_cache::postgres::PgLlmCache, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
    llm::gateway::CallSettings,
    qualification::{QualificationReport, QualificationSettings, render_signal},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain, seen_lead};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

#[derive(Clone, Copy)]
enum Cite {
    Verbatim,
    Fabricated,
    Empty,
    OutOfRange,
}

struct ScriptedProvider {
    input_tokens: u64,
    calls: AtomicU64,
    script: Mutex<VecDeque<Cite>>,
    instructions: Mutex<Vec<String>>,
}

impl ScriptedProvider {
    fn new(input_tokens: u64) -> Self {
        Self::scripted(input_tokens, &[])
    }

    fn scripted(input_tokens: u64, script: &[Cite]) -> Self {
        Self {
            input_tokens,
            calls: AtomicU64::new(0),
            script: Mutex::new(script.iter().copied().collect()),
            instructions: Mutex::new(Vec::new()),
        }
    }

    fn instructions(&self) -> Vec<String> {
        self.instructions.lock().unwrap().clone()
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::SeqCst)
    }
}

fn first_line(content: &str) -> &str {
    content.lines().next().unwrap_or_default()
}

impl LlmProvider for ScriptedProvider {
    async fn complete(&self, request: &LlmRequest<'_>) -> Result<LlmResponse, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.instructions
            .lock()
            .unwrap()
            .push(request.instruction.to_owned());
        let cite = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Cite::Verbatim);
        let quote = request
            .documents
            .first()
            .map_or("", |document| first_line(document.content));
        let evidence = match cite {
            Cite::Verbatim => json!([{"document_index": 0, "quote": quote}]),
            Cite::Fabricated => json!([
                {"document_index": 0, "quote": quote},
                {"document_index": 0, "quote": "runs Keycloak 3.0 behind a hidden admin console"}
            ]),
            Cite::Empty => json!([]),
            Cite::OutOfRange => json!([{"document_index": 9, "quote": quote}]),
        };
        Ok(LlmResponse {
            output: json!({
                "summary": "Runs an outdated Keycloak",
                "approach_angle": "Offer a managed migration",
                "evidence": evidence,
                "draft_message": {"subject": "Your Keycloak", "body": "Hello, we can help."}
            }),
            model: "scripted".to_owned(),
            usage: TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: 0,
            },
        })
    }
}

fn settings(budget_cents: u32, cents_per_mtok: u32) -> QualificationSettings {
    QualificationSettings {
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
        batch_size: 20,
    }
}

async fn iam_profile(pool: &PgPool) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
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

async fn lead_with_score(pool: &PgPool, profile: ProfileId, host: &str, score: f64) -> Signal {
    let lead = seen_lead(pool, profile, host).await;
    let signal = Signal {
        id: SignalId(common::generate_uuid_v7()),
        lead_id: lead.id,
        kind: SignalKind::parse("product_detected").unwrap(),
        value: Some("keycloak".to_owned()),
        source: SourceName::HttpProbe,
        raw: json!({"version": "21.1"}),
        provenance: Provenance::parse(&format!("https://{host}/realms/master")).unwrap(),
        strength: UnitInterval::new(0.9).unwrap(),
        observed_at: at(10),
        agent: None,
    };
    sqlx::query("UPDATE leads SET score = $2 WHERE id = $1")
        .bind(lead.id.0)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
    with_tx(pool, async |shared| {
        PgSignalRepository::new(&shared).record(&signal).await
    })
    .await
    .unwrap();
    signal
}

async fn qualify(
    pool: &PgPool,
    provider: &ScriptedProvider,
    settings: QualificationSettings,
) -> QualificationReport {
    LodestoneUseCase::new(pool.clone())
        .run_qualification(
            provider,
            &PgLlmCache::new(pool.clone()),
            settings,
            &FixedClock,
        )
        .await
        .unwrap()
}

async fn status(pool: &PgPool, profile: ProfileId, host: &str) -> LeadStatus {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await
    })
    .await
    .unwrap()
    .unwrap()
    .status
}

async fn assessments(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<Assessment> {
    with_tx(pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await?
            .unwrap();
        PgAssessmentRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn signals_of(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<Signal> {
    with_tx(pool, async |shared| {
        let lead = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await?
            .unwrap();
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_below_the_threshold_is_never_sent_to_the_llm(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "low.com", 49.9).await;
    let provider = ScriptedProvider::new(10);

    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 0);
    assert_eq!(report.leads_considered, 0);
    assert!(assessments(&pool, profile, "low.com").await.is_empty());
    assert_eq!(status(&pool, profile, "low.com").await, LeadStatus::New);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_above_the_threshold_gets_a_stored_draft_for_review(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let signal = lead_with_score(&pool, profile, "high.com", 80.0).await;
    let before = signals_of(&pool, profile, "high.com").await;
    let provider = ScriptedProvider::new(10);

    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 1);
    assert_eq!(report.leads_considered, 1);
    assert_eq!(report.qualified, 1);
    assert!(!report.budget_exhausted);
    let stored = assessments(&pool, profile, "high.com").await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].profile_revision, RevisionNumber::new(1).unwrap());
    assert_eq!(stored[0].model, "scripted");
    assert_eq!(stored[0].prompt_version, "qualification@1");
    assert_eq!(stored[0].summary, "Runs an outdated Keycloak");
    assert_eq!(stored[0].approach_angle, "Offer a managed migration");
    let draft = stored[0].draft.as_ref().unwrap();
    assert_eq!(draft.subject, "Your Keycloak");
    assert_eq!(draft.body, "Hello, we can help.");
    assert_eq!(stored[0].evidence.len(), 1);
    assert_eq!(
        stored[0].evidence[0].quote,
        first_line(&render_signal(&signal))
    );
    assert_eq!(stored[0].evidence[0].provenance, signal.provenance);
    assert_eq!(
        status(&pool, profile, "high.com").await,
        LeadStatus::Qualified
    );
    assert_eq!(signals_of(&pool, profile, "high.com").await, before);

    let recorded = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].source, SourceName::LlmQualification);
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_run_does_not_qualify_again(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::new(10);

    qualify(&pool, &provider, settings(100, 0)).await;
    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 1);
    assert_eq!(report.leads_considered, 0);
    assert_eq!(assessments(&pool, profile, "high.com").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_domain_is_never_qualified(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "optout.com", 90.0).await;
    LodestoneUseCase::new(pool.clone())
        .add_suppression(
            SuppressionTarget::Domain(domain("optout.com")),
            Some("asked to be left alone".to_owned()),
            None,
        )
        .await
        .unwrap();
    let provider = ScriptedProvider::new(10);

    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 0);
    assert_eq!(report.skipped_suppressed, 1);
    assert_eq!(report.qualified, 0);
    assert!(assessments(&pool, profile, "optout.com").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_exhausted_budget_stops_the_run_after_storing_what_was_done(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "first.com", 90.0).await;
    lead_with_score(&pool, profile, "second.com", 80.0).await;
    let provider = ScriptedProvider::new(1_000_000);

    let report = qualify(&pool, &provider, settings(1, 100)).await;

    assert_eq!(provider.calls(), 1);
    assert!(report.budget_exhausted);
    assert_eq!(report.qualified, 1);
    assert_eq!(assessments(&pool, profile, "first.com").await.len(), 1);
    assert!(assessments(&pool, profile, "second.com").await.is_empty());
    assert_eq!(status(&pool, profile, "second.com").await, LeadStatus::New);
    let recorded = with_tx(&pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Failed);
    assert_eq!(recorded[0].error.as_deref(), Some("llm budget exhausted"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_new_profile_revision_qualifies_the_lead_again(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::new(10);
    qualify(&pool, &provider, settings(100, 0)).await;

    let mut revised: Value = serde_json::from_str(IAM).unwrap();
    revised["identity"]["offer"] = json!("A revised offer");
    LodestoneUseCase::new(pool.clone())
        .revise_profile(ReviseProfileCommand {
            profile_id: profile,
            definition: ValidatedProfile::parse(revised).unwrap(),
            created_by: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET status = 'new'")
        .execute(&pool)
        .await
        .unwrap();

    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert_eq!(report.qualified, 1);
    let stored = assessments(&pool, profile, "high.com").await;
    assert_eq!(stored.len(), 2);
    assert!(
        stored
            .iter()
            .any(|a| a.profile_revision == RevisionNumber::new(2).unwrap())
    );
}

async fn count_assessments(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM assessments")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn fabricated_evidence_is_retried_once_and_the_good_answer_replaces_the_cache(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let signal = lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::scripted(10, &[Cite::Fabricated, Cite::Verbatim]);

    let report = qualify(&pool, &provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert_eq!(report.qualified, 1);
    assert_eq!(report.uncertain, 0);
    let instructions = provider.instructions();
    assert!(!instructions[0].contains("Quote only exact substrings"));
    assert!(instructions[1].ends_with("Quote only exact substrings of the documents."));
    let stored = assessments(&pool, profile, "high.com").await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].evidence.len(), 1);
    assert_eq!(
        stored[0].evidence[0].quote,
        first_line(&render_signal(&signal))
    );
    assert_eq!(
        status(&pool, profile, "high.com").await,
        LeadStatus::Qualified
    );

    sqlx::query("DELETE FROM assessments")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE leads SET status = 'new'")
        .execute(&pool)
        .await
        .unwrap();
    let again = qualify(&pool, &provider, settings(100, 0)).await;
    assert_eq!(provider.calls(), 2);
    assert_eq!(again.qualified, 1);
    assert_eq!(assessments(&pool, profile, "high.com").await.len(), 1);
}

async fn assert_uncertain(pool: &PgPool, profile: ProfileId, provider: &ScriptedProvider) {
    let report = qualify(pool, provider, settings(100, 0)).await;

    assert_eq!(provider.calls(), 2);
    assert_eq!(report.uncertain, 1);
    assert_eq!(report.qualified, 0);
    assert_eq!(count_assessments(pool).await, 0);
    assert_eq!(
        status(pool, profile, "high.com").await,
        LeadStatus::Uncertain
    );
    let qualified: i64 =
        sqlx::query_scalar("SELECT count(*) FROM leads WHERE status = 'qualified'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(qualified, 0);

    let rerun = qualify(pool, provider, settings(100, 0)).await;
    assert_eq!(provider.calls(), 2);
    assert_eq!(rerun.leads_considered, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn evidence_fabricated_twice_marks_the_lead_uncertain(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::scripted(10, &[Cite::Fabricated, Cite::Fabricated]);
    assert_uncertain(&pool, profile, &provider).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn empty_evidence_twice_marks_the_lead_uncertain(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::scripted(10, &[Cite::Empty, Cite::Empty]);
    assert_uncertain(&pool, profile, &provider).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_out_of_range_document_index_counts_as_fabricated(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::scripted(10, &[Cite::OutOfRange, Cite::OutOfRange]);
    assert_uncertain(&pool, profile, &provider).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn budget_exhaustion_on_the_retry_leaves_the_lead_new(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    lead_with_score(&pool, profile, "high.com", 80.0).await;
    let provider = ScriptedProvider::scripted(1_000_000, &[Cite::Fabricated, Cite::Verbatim]);

    let report = qualify(&pool, &provider, settings(1, 100)).await;

    assert_eq!(provider.calls(), 1);
    assert!(report.budget_exhausted);
    assert_eq!(count_assessments(&pool).await, 0);
    assert_eq!(status(&pool, profile, "high.com").await, LeadStatus::New);
}
