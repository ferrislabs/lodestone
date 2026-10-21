mod support;

use chrono::{DateTime, Utc};
use lodestone_core::{
    CLOSED_COMPANY_REASON, CompanyDossier, CompanyRepository, CreateProfileCommand, DossierState,
    Job, JobId, JobKind, JobStatus, LeadQuery, LodestoneUseCase, OfficialData, ProfileId,
    ProfileName, ReviseProfileCommand, RevisionNumber, Siren, ValidatedProfile,
    infrastructure::{company::postgres::PgCompanyRepository, postgres::with_tx},
    jobs::JobError,
    targeting::{TARGETING_SWEEP_JOB_KIND, TargetSweepReport, profile_of_job, sweep_job},
};
use serde_json::{Value, from_str, json};
use sqlx::{PgPool, query, query_scalar};
use support::{at, domain, seen_lead};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const OUT_OF_COUNTRY: &str = "hors pays visé";

fn definition(rules: &Value) -> ValidatedProfile {
    let mut value: Value = from_str(IAM).unwrap();
    if !rules.is_null() {
        value["target_companies"] = rules.clone();
    }
    ValidatedProfile::parse(value).unwrap()
}

async fn profile(usecase: &LodestoneUseCase, name: &str, rules: &Value, active: bool) -> ProfileId {
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: definition(rules),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    if active {
        usecase.set_profile_active(id, true).await.unwrap();
    }
    id
}

async fn revise(usecase: &LodestoneUseCase, id: ProfileId, rules: &Value) {
    usecase
        .revise_profile(ReviseProfileCommand {
            profile_id: id,
            definition: definition(rules),
            created_by: None,
        })
        .await
        .unwrap();
}

fn official(naf: &str, headcount: &str) -> OfficialData {
    OfficialData {
        siren: Siren::parse("552081317").unwrap(),
        legal_name: "ACME SAS".to_owned(),
        naf_code: Some(naf.to_owned()),
        naf_label: None,
        headcount_code: Some(headcount.to_owned()),
        headcount_range: None,
        head_office_city: Some("LYON".to_owned()),
        head_office_postcode: Some("69002".to_owned()),
        head_office_country: Some("France".to_owned()),
        head_office_country_code: Some("FR".to_owned()),
        created_on: None,
        active: Some(true),
        category: None,
        legal_nature: None,
        head_office_department: Some("69".to_owned()),
    }
}

async fn dossier(pool: &PgPool, host: &str, official: Option<OfficialData>) -> CompanyDossier {
    let mut dossier = CompanyDossier::pending(domain(host), at(0));
    dossier.state = DossierState::Complete;
    dossier.siren = official.as_ref().map(|data| data.siren.clone());
    dossier.official = official;
    with_tx(pool, async |shared| {
        PgCompanyRepository::new(&shared).save(&dossier).await
    })
    .await
    .unwrap();
    dossier
}

async fn exclusion(pool: &PgPool, profile: ProfileId, host: &str) -> Option<String> {
    query_scalar("SELECT excluded_reason FROM leads WHERE profile_id = $1 AND domain = $2")
        .bind(profile.0)
        .bind(host)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn events(pool: &PgPool, profile: ProfileId, host: &str, kind: &str) -> Vec<String> {
    query_scalar(
        "SELECT COALESCE(lead_events.detail ->> 'reason', '') FROM lead_events JOIN leads ON leads.id = lead_events.lead_id WHERE leads.profile_id = $1 AND leads.domain = $2 AND lead_events.kind = $3 ORDER BY lead_events.started_at, lead_events.id",
    )
    .bind(profile.0)
    .bind(host)
    .bind(kind)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn listed(usecase: &LodestoneUseCase, profile: ProfileId) -> u64 {
    usecase
        .list_leads(
            profile,
            LeadQuery {
                limit: 50,
                ..LeadQuery::default()
            },
        )
        .await
        .unwrap()
        .page
        .total
}

async fn force_exclusion(pool: &PgPool, profile: ProfileId, host: &str, reason: &str) {
    query("UPDATE leads SET excluded_reason = $3, excluded_at = $4 WHERE profile_id = $1 AND domain = $2")
        .bind(profile.0)
        .bind(host)
        .bind(reason)
        .bind(at(1))
        .execute(pool)
        .await
        .unwrap();
}

fn staffing() -> Value {
    json!({"naf_exclude": ["78"]})
}

fn report(excluded: u64, restored: u64) -> TargetSweepReport {
    TargetSweepReport { excluded, restored }
}

async fn sweep(usecase: &LodestoneUseCase, profile: ProfileId, seconds: i64) -> TargetSweepReport {
    usecase
        .sweep_target_companies(profile, at(seconds))
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_saved_dossier_outside_the_naf_target_excludes_the_lead_and_records_it(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let strict = profile(&usecase, "strict", &staffing(), true).await;
    let open = profile(&usecase, "open", &Value::Null, true).await;
    let strict_lead = seen_lead(&pool, strict, "acme.com").await;
    seen_lead(&pool, open, "acme.com").await;
    let saved = dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;

    let excluded = usecase.exclude_off_target(&saved, at(10)).await.unwrap();

    assert_eq!(excluded, 1);
    assert_eq!(
        exclusion(&pool, strict, "acme.com").await.as_deref(),
        Some("hors cible : activité 78.20Z")
    );
    assert_eq!(exclusion(&pool, open, "acme.com").await, None);
    assert_eq!(listed(&usecase, strict).await, 0);
    assert_eq!(listed(&usecase, open).await, 1);
    assert!(usecase.lead_detail(strict_lead.id).await.is_err());
    assert_eq!(
        events(&pool, strict, "acme.com", "excluded").await,
        ["hors cible : activité 78.20Z"]
    );
    assert!(events(&pool, open, "acme.com", "excluded").await.is_empty());
    assert_eq!(usecase.exclude_off_target(&saved, at(11)).await.unwrap(), 0);
    assert_eq!(events(&pool, strict, "acme.com", "excluded").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_closed_company_is_excluded_for_a_profile_without_rules(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let open = profile(&usecase, "open", &Value::Null, true).await;
    seen_lead(&pool, open, "acme.com").await;
    seen_lead(&pool, open, "alive.com").await;
    let mut closed = official("62.01Z", "12");
    closed.active = Some(false);
    let saved = dossier(&pool, "acme.com", Some(closed)).await;
    let alive = dossier(&pool, "alive.com", Some(official("62.01Z", "12"))).await;

    assert_eq!(usecase.exclude_off_target(&saved, at(10)).await.unwrap(), 1);
    assert_eq!(usecase.exclude_off_target(&alive, at(10)).await.unwrap(), 0);

    assert_eq!(
        exclusion(&pool, open, "acme.com").await.as_deref(),
        Some(CLOSED_COMPANY_REASON)
    );
    assert_eq!(exclusion(&pool, open, "alive.com").await, None);
    assert_eq!(listed(&usecase, open).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_of_other_domains_inactive_profiles_and_other_reasons_are_left_alone(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let strict = profile(&usecase, "strict", &staffing(), true).await;
    let dormant = profile(&usecase, "dormant", &staffing(), false).await;
    seen_lead(&pool, strict, "acme.com").await;
    seen_lead(&pool, strict, "other.com").await;
    seen_lead(&pool, strict, "abroad.com").await;
    seen_lead(&pool, dormant, "acme.com").await;
    force_exclusion(&pool, strict, "abroad.com", OUT_OF_COUNTRY).await;
    let saved = dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;
    let abroad = dossier(&pool, "abroad.com", Some(official("78.20Z", "12"))).await;

    assert_eq!(usecase.exclude_off_target(&saved, at(10)).await.unwrap(), 1);
    assert_eq!(
        usecase.exclude_off_target(&abroad, at(10)).await.unwrap(),
        0
    );

    assert_eq!(exclusion(&pool, strict, "other.com").await, None);
    assert_eq!(exclusion(&pool, dormant, "acme.com").await, None);
    assert_eq!(
        exclusion(&pool, strict, "abroad.com").await.as_deref(),
        Some(OUT_OF_COUNTRY)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_dossier_without_official_data_excludes_nothing(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let strict = profile(&usecase, "strict", &staffing(), true).await;
    seen_lead(&pool, strict, "acme.com").await;
    let saved = dossier(&pool, "acme.com", None).await;

    assert_eq!(usecase.exclude_off_target(&saved, at(10)).await.unwrap(), 0);

    assert_eq!(exclusion(&pool, strict, "acme.com").await, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_lead_gets_the_reason_of_its_own_profile(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let by_activity = profile(&usecase, "activity", &staffing(), true).await;
    let by_size = profile(&usecase, "size", &json!({"min_employees": 50}), true).await;
    seen_lead(&pool, by_activity, "acme.com").await;
    seen_lead(&pool, by_size, "acme.com").await;
    let saved = dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;

    assert_eq!(usecase.exclude_off_target(&saved, at(10)).await.unwrap(), 2);

    assert_eq!(
        exclusion(&pool, by_activity, "acme.com").await.as_deref(),
        Some("hors cible : activité 78.20Z")
    );
    assert_eq!(
        exclusion(&pool, by_size, "acme.com").await.as_deref(),
        Some("hors cible : effectif 12")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_new_revision_queues_one_sweep_and_an_unchanged_one_none(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "strict", &Value::Null, true).await;
    let other = profile(&usecase, "other", &Value::Null, true).await;

    let pending = |kind: &'static str| {
        let pool = pool.clone();
        async move {
            query_scalar::<_, String>(
                "SELECT dedup_key FROM jobs WHERE kind = $1 AND status = 'pending' ORDER BY dedup_key",
            )
            .bind(kind)
            .fetch_all(&pool)
            .await
            .unwrap()
        }
    };
    let sorted = |mut keys: Vec<String>| {
        keys.sort();
        keys
    };
    assert_eq!(
        pending(TARGETING_SWEEP_JOB_KIND).await,
        sorted(vec![
            format!("targeting:sweep:{id}:1"),
            format!("targeting:sweep:{other}:1"),
        ])
    );

    revise(&usecase, id, &staffing()).await;
    revise(&usecase, id, &json!({"min_employees": 5})).await;
    revise(&usecase, id, &json!({"min_employees": 5})).await;
    assert_eq!(
        pending(TARGETING_SWEEP_JOB_KIND).await,
        sorted(vec![
            format!("targeting:sweep:{id}:1"),
            format!("targeting:sweep:{id}:2"),
            format!("targeting:sweep:{id}:3"),
            format!("targeting:sweep:{other}:1"),
        ])
    );

    let payload: Value = query_scalar("SELECT payload FROM jobs WHERE dedup_key = $1")
        .bind(format!("targeting:sweep:{id}:3"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload, json!({"profile_id": id.to_string()}));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revision_that_removes_the_rule_restores_the_lead(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "strict", &staffing(), true).await;
    seen_lead(&pool, id, "acme.com").await;
    let saved = dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;
    usecase.exclude_off_target(&saved, at(10)).await.unwrap();
    assert_eq!(listed(&usecase, id).await, 0);

    revise(&usecase, id, &Value::Null).await;
    let swept = sweep(&usecase, id, 20).await;

    assert_eq!(swept, report(0, 1));
    assert_eq!(exclusion(&pool, id, "acme.com").await, None);
    assert_eq!(listed(&usecase, id).await, 1);
    assert_eq!(
        events(&pool, id, "acme.com", "exclusion_lifted").await,
        ["hors cible : activité 78.20Z"]
    );
    assert_eq!(sweep(&usecase, id, 30).await, report(0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_revision_that_adds_a_rule_excludes_the_existing_lead(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "open", &Value::Null, true).await;
    seen_lead(&pool, id, "acme.com").await;
    seen_lead(&pool, id, "fine.com").await;
    seen_lead(&pool, id, "unknown.com").await;
    dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;
    dossier(&pool, "fine.com", Some(official("62.01Z", "12"))).await;
    dossier(&pool, "unknown.com", None).await;
    assert_eq!(sweep(&usecase, id, 10).await, report(0, 0));

    revise(&usecase, id, &staffing()).await;
    let swept = sweep(&usecase, id, 20).await;

    assert_eq!(swept, report(1, 0));
    assert_eq!(
        exclusion(&pool, id, "acme.com").await.as_deref(),
        Some("hors cible : activité 78.20Z")
    );
    assert_eq!(exclusion(&pool, id, "fine.com").await, None);
    assert_eq!(exclusion(&pool, id, "unknown.com").await, None);
    assert_eq!(listed(&usecase, id).await, 2);
    assert_eq!(
        events(&pool, id, "acme.com", "excluded").await,
        ["hors cible : activité 78.20Z"]
    );
    assert_eq!(sweep(&usecase, id, 30).await, report(0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_changed_rule_replaces_the_stored_target_reason(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "strict", &staffing(), true).await;
    seen_lead(&pool, id, "acme.com").await;
    let saved = dossier(&pool, "acme.com", Some(official("78.20Z", "12"))).await;
    usecase.exclude_off_target(&saved, at(10)).await.unwrap();

    revise(&usecase, id, &json!({"min_employees": 50})).await;
    let swept = sweep(&usecase, id, 20).await;

    assert_eq!(swept, report(1, 0));
    assert_eq!(
        exclusion(&pool, id, "acme.com").await.as_deref(),
        Some("hors cible : effectif 12")
    );
    assert_eq!(listed(&usecase, id).await, 0);
    assert_eq!(sweep(&usecase, id, 30).await, report(0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_sweep_never_touches_a_lead_excluded_for_another_reason(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "open", &Value::Null, true).await;
    seen_lead(&pool, id, "abroad.com").await;
    seen_lead(&pool, id, "saas.com").await;
    dossier(&pool, "abroad.com", Some(official("62.01Z", "12"))).await;
    dossier(&pool, "saas.com", Some(official("78.20Z", "12"))).await;
    force_exclusion(&pool, id, "abroad.com", OUT_OF_COUNTRY).await;
    force_exclusion(&pool, id, "saas.com", "domaine d'un fournisseur SaaS").await;

    assert_eq!(sweep(&usecase, id, 10).await, report(0, 0));
    revise(&usecase, id, &staffing()).await;
    assert_eq!(sweep(&usecase, id, 20).await, report(0, 0));
    revise(&usecase, id, &Value::Null).await;
    assert_eq!(sweep(&usecase, id, 30).await, report(0, 0));

    assert_eq!(
        exclusion(&pool, id, "abroad.com").await.as_deref(),
        Some(OUT_OF_COUNTRY)
    );
    assert_eq!(
        exclusion(&pool, id, "saas.com").await.as_deref(),
        Some("domaine d'un fournisseur SaaS")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_sweep_restores_a_company_that_reopened_and_only_for_its_own_profile(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "open", &Value::Null, true).await;
    let other = profile(&usecase, "other", &staffing(), true).await;
    seen_lead(&pool, id, "acme.com").await;
    seen_lead(&pool, other, "acme.com").await;
    let mut closed = official("62.01Z", "12");
    closed.active = Some(false);
    let saved = dossier(&pool, "acme.com", Some(closed)).await;
    assert_eq!(usecase.exclude_off_target(&saved, at(10)).await.unwrap(), 2);
    dossier(&pool, "acme.com", Some(official("62.01Z", "12"))).await;

    assert_eq!(sweep(&usecase, id, 20).await, report(0, 1));

    assert_eq!(exclusion(&pool, id, "acme.com").await, None);
    assert_eq!(
        exclusion(&pool, other, "acme.com").await.as_deref(),
        Some(CLOSED_COMPANY_REASON)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_sweep_walks_every_batch(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = profile(&usecase, "strict", &staffing(), true).await;
    for index in 0..7 {
        let host = format!("acme{index}.com");
        seen_lead(&pool, id, &host).await;
        let naf = if index % 2 == 0 { "78.20Z" } else { "62.01Z" };
        dossier(&pool, &host, Some(official(naf, "12"))).await;
    }

    let swept = usecase
        .sweep_target_companies_by(id, at(10), 3)
        .await
        .unwrap();

    assert_eq!(swept, report(4, 0));
    assert_eq!(listed(&usecase, id).await, 3);
    revise(&usecase, id, &Value::Null).await;
    let restored = usecase
        .sweep_target_companies_by(id, at(20), 3)
        .await
        .unwrap();
    assert_eq!(restored, report(0, 4));
    assert_eq!(listed(&usecase, id).await, 7);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sweeping_an_unknown_profile_does_nothing(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool);
    let unknown = ProfileId(uuid::Uuid::nil());
    assert_eq!(sweep(&usecase, unknown, 10).await, report(0, 0));
}

fn sweep_row(payload: Value) -> Job {
    Job {
        id: JobId(uuid::Uuid::nil()),
        kind: JobKind::parse(TARGETING_SWEEP_JOB_KIND).unwrap(),
        payload,
        status: JobStatus::Running,
        attempts: 1,
        max_attempts: std::num::NonZeroU16::new(3).unwrap(),
        run_at: DateTime::<Utc>::MIN_UTC,
        last_error: None,
        created_at: DateTime::<Utc>::MIN_UTC,
        updated_at: DateTime::<Utc>::MIN_UTC,
    }
}

#[test]
fn the_sweep_job_is_keyed_by_profile_and_revision_and_carries_the_profile() {
    let id = ProfileId(uuid::Uuid::from_u128(7));
    let other = ProfileId(uuid::Uuid::from_u128(8));
    let first = RevisionNumber::FIRST;
    let second = RevisionNumber::new(2).unwrap();
    let job = sweep_job(id, first, at(0)).unwrap();
    assert_eq!(job.kind.as_str(), "targeting.sweep");
    assert_eq!(job.kind.as_str(), TARGETING_SWEEP_JOB_KIND);
    assert_eq!(job.dedup_key, Some(format!("targeting:sweep:{id}:1")));
    assert_ne!(
        job.dedup_key,
        sweep_job(other, first, at(0)).unwrap().dedup_key
    );
    assert_ne!(
        job.dedup_key,
        sweep_job(id, second, at(0)).unwrap().dedup_key
    );
    assert_eq!(job.run_at, at(0));
    assert_eq!(profile_of_job(&sweep_row(job.payload)), Ok(id));
}

#[test]
fn a_sweep_job_without_a_valid_profile_is_a_permanent_failure() {
    for payload in [
        json!({}),
        json!({"profile_id": 3}),
        json!({"profile_id": "nope"}),
    ] {
        assert!(matches!(
            profile_of_job(&sweep_row(payload)),
            Err(JobError::Permanent(_))
        ));
    }
}
