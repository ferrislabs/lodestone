mod support;

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, Candidate, Clock, CreateProfileCommand, DropReason, DropReview,
    DroppedCandidate, DroppedCandidateRepository, DroppedFilter, DroppedId, LeadRepository,
    LodestoneUseCase, NormalizedDomain, ProfileId, ProfileName, Provenance, SignalKind,
    SignalRepository, SourceError, SourceName, UserId, ValidatedProfile,
    definition::AgentId,
    dropped::{
        DROPPED_PURGE_JOB_KIND, DROPPED_RETENTION_DAYS, dropped_purge_job, next_dropped_purge,
    },
    infrastructure::{
        dropped::postgres::PgDroppedCandidateRepository, lead::postgres::PgLeadRepository,
        postgres::with_tx, signal::postgres::PgSignalRepository,
    },
    triage::{CandidateSource, MAX_DROPPED_PER_TARGET, TriageReport},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{at, domain};
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct TickClock(i64);

impl Clock for TickClock {
    fn now(&self) -> DateTime<Utc> {
        at(self.0)
    }
}

struct FakeSource {
    source: SourceName,
    candidates: Vec<Candidate>,
}

impl CandidateSource for FakeSource {
    fn name(&self) -> SourceName {
        self.source
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

fn offer(host: &str, title: &str, text: &str) -> Candidate {
    Candidate {
        domain: domain(host),
        source: SourceName::Boamp,
        kind: SignalKind::parse("job_offer").unwrap(),
        title: title.to_owned(),
        text: text.to_owned(),
        provenance: Provenance::parse(&format!("https://jobs.example/{host}")).unwrap(),
        observed_at: at(50),
        details: json!({"id": host}),
    }
}

fn boamp(candidates: Vec<Candidate>) -> FakeSource {
    FakeSource {
        source: SourceName::Boamp,
        candidates,
    }
}

fn jobs_agent(id: &str) -> Value {
    json!({
        "type": "search",
        "id": id,
        "name": id,
        "enabled": true,
        "providers": ["boamp"],
        "keywords": ["IAM"],
        "exclusions": ["stagiaire"],
        "accept_min_keywords": 1,
        "filters": {
            "remote": "remote_or_hybrid",
            "countries": ["FR"],
            "contracts": ["freelance"],
        },
    })
}

async fn profile_with(pool: &PgPool, name: &str, agents: Vec<Value>) -> ProfileId {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["agents"] = Value::Array(agents);
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

async fn triage(pool: &PgPool, source: &FakeSource, clock: i64) -> TriageReport {
    LodestoneUseCase::new(pool.clone())
        .run_triage(source, &TickClock(clock))
        .await
        .unwrap()
}

async fn all_dropped(pool: &PgPool, profile: ProfileId) -> Vec<DroppedCandidate> {
    LodestoneUseCase::new(pool.clone())
        .list_dropped(profile, &DroppedFilter::default(), 100, false)
        .await
        .unwrap()
}

fn dropped_of<'a>(rows: &'a [DroppedCandidate], host: &str) -> &'a DroppedCandidate {
    rows.iter()
        .find(|row| row.domain.as_str() == host)
        .unwrap_or_else(|| panic!("no dropped candidate for {host}"))
}

fn actor() -> UserId {
    UserId(Uuid::now_v7())
}

async fn signals_of(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
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

async fn has_lead(pool: &PgPool, profile: ProfileId, host: &str) -> bool {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await
    })
    .await
    .unwrap()
    .is_some()
}

fn five_reasons() -> Vec<Candidate> {
    vec![
        offer(
            "nokeyword.fr",
            "Comptable",
            "Poste de comptable, freelance, full remote",
        ),
        offer(
            "excluded.fr",
            "Stagiaire IAM",
            "Un stagiaire pour le projet IAM, freelance, full remote",
        ),
        offer(
            "onsite.fr",
            "Mission IAM",
            "Mission freelance IAM, sur site à Lyon",
        ),
        offer("abroad.de", "Mission IAM", "Freelance IAM, full remote"),
        offer("employee.fr", "Poste IAM", "CDI IAM, full remote"),
        offer("kept.fr", "Mission IAM", "Freelance IAM, full remote"),
    ]
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_run_keeps_one_dropped_candidate_per_reason_with_its_passages(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;

    let report = triage(&pool, &boamp(five_reasons()), 100).await;

    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.rejected_by_exclusions, 1);
    assert_eq!(report.rejected_by_filters, 3);
    assert_eq!(report.dropped_kept, 5);
    assert_eq!(report.dropped_not_kept, 0);
    let rows = all_dropped(&pool, profile).await;
    assert_eq!(rows.len(), 5);
    assert!(rows.iter().all(|row| row.domain.as_str() != "kept.fr"));
    assert!(rows.iter().all(|row| row.review.is_none()));
    assert!(rows.iter().all(|row| row.profile_id == profile));
    assert!(
        rows.iter()
            .all(|row| row.agent == Some(AgentId::new("jobs")))
    );
    assert!(rows.iter().all(|row| row.source == SourceName::Boamp));
    assert!(rows.iter().all(|row| row.dropped_at == at(100)));

    let no_keyword = dropped_of(&rows, "nokeyword.fr");
    assert_eq!(no_keyword.reason, DropReason::NoKeyword);
    assert!(no_keyword.matched.is_empty());
    assert!(no_keyword.excluded.is_empty());
    assert_eq!(no_keyword.title, "Comptable");
    assert_eq!(
        no_keyword.excerpts,
        ["Poste de comptable, freelance, full remote"]
    );
    assert_eq!(
        no_keyword.provenance.as_str(),
        "https://jobs.example/nokeyword.fr"
    );
    assert_eq!(no_keyword.observed_at, at(50));
    assert_eq!(no_keyword.kind.as_str(), "job_offer");
    assert_eq!(no_keyword.details, json!({"id": "nokeyword.fr"}));

    let excluded = dropped_of(&rows, "excluded.fr");
    assert_eq!(excluded.reason, DropReason::Exclusion);
    assert_eq!(excluded.matched, ["IAM"]);
    assert_eq!(excluded.excluded, ["stagiaire"]);
    assert_eq!(excluded.excerpts.len(), 1);
    assert!(excluded.excerpts[0].contains("Un stagiaire pour le projet IAM"));

    let remote = dropped_of(&rows, "onsite.fr");
    assert_eq!(remote.reason, DropReason::FilterRemote);
    assert_eq!(remote.matched, ["IAM"]);
    assert!(remote.excluded.is_empty());
    assert!(remote.excerpts[0].contains("Mission freelance IAM, sur site"));

    let country = dropped_of(&rows, "abroad.de");
    assert_eq!(country.reason, DropReason::FilterCountry);
    assert_eq!(country.matched, ["IAM"]);

    let contract = dropped_of(&rows, "employee.fr");
    assert_eq!(contract.reason, DropReason::FilterContract);
    assert_eq!(contract.matched, ["IAM"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_filter_drop_without_any_keyword_opens_with_the_start_of_the_text(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let source = boamp(vec![offer(
        "onsite.fr",
        "Comptable",
        "Comptable  confirmé\n\nsur site à Lyon",
    )]);

    triage(&pool, &source, 100).await;

    let rows = all_dropped(&pool, profile).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, DropReason::FilterRemote);
    assert!(rows[0].matched.is_empty());
    assert_eq!(rows[0].excerpts, ["Comptable confirmé sur site à Lyon"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn running_the_same_triage_again_refreshes_the_rows_and_keeps_their_review(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let source = boamp(five_reasons());
    triage(&pool, &source, 100).await;
    let first = all_dropped(&pool, profile).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let reviewed = dropped_of(&first, "nokeyword.fr");
    usecase
        .review_dropped(reviewed.id, DropReview::RightlyDropped, actor(), at(150))
        .await
        .unwrap();

    let mut changed = five_reasons();
    changed[0].title = "Comptable senior".to_owned();
    triage(&pool, &boamp(changed), 200).await;

    let second = all_dropped(&pool, profile).await;
    assert_eq!(second.len(), 5);
    let refreshed = dropped_of(&second, "nokeyword.fr");
    assert_eq!(refreshed.id, reviewed.id);
    assert_eq!(refreshed.title, "Comptable senior");
    assert_eq!(refreshed.dropped_at, at(200));
    assert_eq!(refreshed.review, Some(DropReview::RightlyDropped));
    assert_eq!(refreshed.reviewed_at, Some(at(150)));
    let untouched = dropped_of(&second, "excluded.fr");
    assert_eq!(untouched.id, dropped_of(&first, "excluded.fr").id);
    assert_eq!(untouched.dropped_at, at(200));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_changed_reason_replaces_the_stored_one(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(
        &pool,
        &boamp(vec![offer("acme.fr", "Poste", "Poste de comptable")]),
        100,
    )
    .await;
    triage(
        &pool,
        &boamp(vec![offer(
            "acme.fr",
            "Poste IAM",
            "Stagiaire IAM, freelance full remote",
        )]),
        200,
    )
    .await;

    let rows = all_dropped(&pool, profile).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, DropReason::Exclusion);
    assert_eq!(rows[0].excluded, ["stagiaire"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_keep_their_own_dropped_candidates_for_the_same_offer(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "jobs",
        vec![jobs_agent("first"), jobs_agent("second")],
    )
    .await;
    let source = boamp(vec![offer("acme.fr", "Comptable", "Poste de comptable")]);

    let report = triage(&pool, &source, 100).await;

    assert_eq!(report.dropped_kept, 2);
    let rows = all_dropped(&pool, profile).await;
    assert_eq!(rows.len(), 2);
    let mut agents: Vec<_> = rows
        .iter()
        .filter_map(|row| row.agent.as_ref().map(|agent| agent.as_str().to_owned()))
        .collect();
    agents.sort();
    assert_eq!(agents, ["first", "second"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn at_most_two_hundred_dropped_candidates_are_kept_per_agent_and_run(pool: PgPool) {
    assert_eq!(MAX_DROPPED_PER_TARGET, 200);
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let candidates: Vec<Candidate> = (0..250)
        .map(|index| {
            offer(
                &format!("h{index}.fr"),
                "Comptable",
                "Poste de comptable, freelance, full remote",
            )
        })
        .collect();

    let report = triage(&pool, &boamp(candidates), 100).await;

    assert_eq!(report.candidates, 250);
    assert_eq!(report.rejected_by_rules, 250);
    assert_eq!(report.dropped_kept, 200);
    assert_eq!(report.dropped_not_kept, 50);
    let rows = LodestoneUseCase::new(pool.clone())
        .list_dropped(profile, &DroppedFilter::default(), 100, false)
        .await
        .unwrap();
    assert_eq!(rows.len(), 100);
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dropped_candidates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 200);
    let hosts: Vec<String> = sqlx::query_scalar("SELECT domain FROM dropped_candidates")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(hosts.contains(&"h199.fr".to_owned()));
    assert!(!hosts.contains(&"h200.fr".to_owned()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_run_with_nothing_dropped_stores_nothing(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let source = boamp(vec![offer(
        "kept.fr",
        "Mission IAM",
        "Freelance IAM, full remote",
    )]);

    let report = triage(&pool, &source, 100).await;

    assert_eq!(report.dropped_kept, 0);
    assert_eq!(report.dropped_not_kept, 0);
    assert!(all_dropped(&pool, profile).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_storage_failure_does_not_fail_the_run_nor_lose_the_accepted_candidates(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    sqlx::query("DROP TABLE dropped_candidates")
        .execute(&pool)
        .await
        .unwrap();

    let report = triage(&pool, &boamp(five_reasons()), 100).await;

    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.dropped_kept, 0);
    assert_eq!(report.dropped_not_kept, 5);
    assert_eq!(signals_of(&pool, profile, "kept.fr").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn listing_filters_by_reason_agent_and_review(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "jobs",
        vec![jobs_agent("first"), jobs_agent("second")],
    )
    .await;
    triage(&pool, &boamp(five_reasons()), 100).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let rows = all_dropped(&pool, profile).await;
    assert_eq!(rows.len(), 10);

    let by_reason = DroppedFilter {
        reason: Some(DropReason::Exclusion),
        ..DroppedFilter::default()
    };
    let exclusions = usecase
        .list_dropped(profile, &by_reason, 100, false)
        .await
        .unwrap();
    assert_eq!(exclusions.len(), 2);
    assert!(
        exclusions
            .iter()
            .all(|row| row.reason == DropReason::Exclusion)
    );

    let by_agent = DroppedFilter {
        agent: Some(AgentId::new("second")),
        ..DroppedFilter::default()
    };
    let second = usecase
        .list_dropped(profile, &by_agent, 100, false)
        .await
        .unwrap();
    assert_eq!(second.len(), 5);

    let target = second
        .iter()
        .find(|row| row.reason == DropReason::NoKeyword);
    usecase
        .review_dropped(
            target.unwrap().id,
            DropReview::RightlyDropped,
            actor(),
            at(150),
        )
        .await
        .unwrap();
    let reviewed = DroppedFilter {
        reviewed: Some(true),
        ..DroppedFilter::default()
    };
    assert_eq!(
        usecase
            .list_dropped(profile, &reviewed, 100, false)
            .await
            .unwrap()
            .len(),
        1
    );
    let pending = DroppedFilter {
        reviewed: Some(false),
        ..DroppedFilter::default()
    };
    assert_eq!(
        usecase
            .list_dropped(profile, &pending, 100, false)
            .await
            .unwrap()
            .len(),
        9
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn listing_orders_by_recency_or_at_random_within_the_limit(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(
        &pool,
        &boamp(vec![
            offer("old.fr", "Comptable", "Poste de comptable"),
            offer("older.fr", "Comptable", "Poste de comptable"),
        ]),
        100,
    )
    .await;
    triage(
        &pool,
        &boamp(vec![offer("new.fr", "Comptable", "Poste de comptable")]),
        300,
    )
    .await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let recent = usecase
        .list_dropped(profile, &DroppedFilter::default(), 2, false)
        .await
        .unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].domain.as_str(), "new.fr");

    let random = usecase
        .list_dropped(profile, &DroppedFilter::default(), 2, true)
        .await
        .unwrap();
    assert_eq!(random.len(), 2);
    let all = usecase
        .list_dropped(profile, &DroppedFilter::default(), 10, true)
        .await
        .unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn listing_an_unknown_profile_is_not_found(pool: PgPool) {
    let result = LodestoneUseCase::new(pool.clone())
        .list_dropped(
            ProfileId(Uuid::now_v7()),
            &DroppedFilter::default(),
            10,
            false,
        )
        .await;
    assert!(matches!(result, Err(CoreError::NotFound)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn listing_keeps_the_candidates_of_each_profile_apart(pool: PgPool) {
    let first = profile_with(&pool, "first", vec![jobs_agent("jobs")]).await;
    let second = profile_with(&pool, "second", vec![jobs_agent("jobs")]).await;
    triage(
        &pool,
        &boamp(vec![offer("acme.fr", "Comptable", "Poste de comptable")]),
        100,
    )
    .await;

    assert_eq!(all_dropped(&pool, first).await.len(), 1);
    assert_eq!(all_dropped(&pool, second).await.len(), 1);
    let stored: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dropped_candidates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn recovering_a_dropped_candidate_creates_a_lead_with_a_recovered_signal(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(&pool, &boamp(five_reasons()), 100).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let rows = all_dropped(&pool, profile).await;
    let target = dropped_of(&rows, "onsite.fr");
    assert!(!has_lead(&pool, profile, "onsite.fr").await);
    let reviewer = actor();

    let reviewed = usecase
        .review_dropped(target.id, DropReview::ShouldHaveKept, reviewer, at(500))
        .await
        .unwrap();

    assert_eq!(reviewed.candidate.id, target.id);
    assert_eq!(reviewed.candidate.review, Some(DropReview::ShouldHaveKept));
    assert_eq!(reviewed.candidate.reviewed_at, Some(at(500)));
    assert_eq!(reviewed.candidate.reviewed_by, Some(reviewer));
    assert!(reviewed.lead_id.is_some());
    assert!(has_lead(&pool, profile, "onsite.fr").await);
    let signals = signals_of(&pool, profile, "onsite.fr").await;
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].raw["route"], "recovered");
    assert_eq!(signals[0].raw["title"], "Mission IAM");
    assert_eq!(signals[0].raw["matched"], json!(["IAM"]));
    assert_eq!(signals[0].raw["details"], json!({"id": "onsite.fr"}));
    assert!(signals[0].raw["excerpts"].is_array());
    assert_eq!(signals[0].agent, Some(AgentId::new("jobs")));
    assert_eq!(signals[0].strength.get(), 0.5);
    assert_eq!(signals[0].source, SourceName::Boamp);
    assert_eq!(signals[0].kind.as_str(), "job_offer");
    assert_eq!(signals[0].value.as_deref(), Some("Mission IAM"));
    assert_eq!(
        signals[0].provenance.as_str(),
        "https://jobs.example/onsite.fr"
    );
    assert_eq!(signals[0].observed_at, at(50));

    let stored = all_dropped(&pool, profile).await;
    assert_eq!(
        dropped_of(&stored, "onsite.fr").review,
        Some(DropReview::ShouldHaveKept)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_review_of_the_same_candidate_is_a_conflict(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(&pool, &boamp(five_reasons()), 100).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let rows = all_dropped(&pool, profile).await;
    let target = dropped_of(&rows, "onsite.fr");
    usecase
        .review_dropped(target.id, DropReview::ShouldHaveKept, actor(), at(500))
        .await
        .unwrap();

    for review in [DropReview::ShouldHaveKept, DropReview::RightlyDropped] {
        let again = usecase
            .review_dropped(target.id, review, actor(), at(600))
            .await;
        assert!(matches!(again, Err(CoreError::Conflict(_))));
    }
    assert_eq!(signals_of(&pool, profile, "onsite.fr").await.len(), 1);
    let stored = all_dropped(&pool, profile).await;
    assert_eq!(dropped_of(&stored, "onsite.fr").reviewed_at, Some(at(500)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_drop_creates_no_lead(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(&pool, &boamp(five_reasons()), 100).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let rows = all_dropped(&pool, profile).await;
    let target = dropped_of(&rows, "onsite.fr");

    let reviewed = usecase
        .review_dropped(target.id, DropReview::RightlyDropped, actor(), at(500))
        .await
        .unwrap();

    assert_eq!(reviewed.candidate.review, Some(DropReview::RightlyDropped));
    assert_eq!(reviewed.lead_id, None);
    assert!(!has_lead(&pool, profile, "onsite.fr").await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reviewing_an_unknown_candidate_is_not_found(pool: PgPool) {
    let result = LodestoneUseCase::new(pool.clone())
        .review_dropped(
            DroppedId(Uuid::now_v7()),
            DropReview::RightlyDropped,
            actor(),
            at(0),
        )
        .await;
    assert!(matches!(result, Err(CoreError::NotFound)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn recovering_under_an_inactive_profile_is_a_conflict_and_changes_nothing(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    triage(&pool, &boamp(five_reasons()), 100).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let rows = all_dropped(&pool, profile).await;
    let target = dropped_of(&rows, "onsite.fr");
    usecase.set_profile_active(profile, false).await.unwrap();

    let result = usecase
        .review_dropped(target.id, DropReview::ShouldHaveKept, actor(), at(500))
        .await;

    assert!(matches!(result, Err(CoreError::Conflict(_))));
    assert!(!has_lead(&pool, profile, "onsite.fr").await);
    let stored = all_dropped(&pool, profile).await;
    assert_eq!(dropped_of(&stored, "onsite.fr").review, None);

    let confirmed = usecase
        .review_dropped(target.id, DropReview::RightlyDropped, actor(), at(600))
        .await
        .unwrap();
    assert_eq!(confirmed.candidate.review, Some(DropReview::RightlyDropped));
}

struct CompanyJobs {
    candidates: Vec<Candidate>,
}

impl CandidateSource for CompanyJobs {
    fn name(&self) -> SourceName {
        SourceName::Ats
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

fn ats_offer(host: &str, title: &str, text: &str) -> Candidate {
    Candidate {
        source: SourceName::Ats,
        ..offer(host, title, text)
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_company_job_only_attaches_to_a_lead_that_already_exists(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "iam",
        serde_json::from_str::<Value>(IAM).unwrap()["agents"]
            .as_array()
            .unwrap()
            .clone(),
    )
    .await;
    let usecase = LodestoneUseCase::new(pool.clone());
    let source = CompanyJobs {
        candidates: vec![
            ats_offer("known.fr", "Comptable", "Poste de comptable"),
            ats_offer("unknown.fr", "Comptable", "Poste de comptable"),
        ],
    };
    support::seen_lead(&pool, profile, "known.fr").await;
    usecase.run_triage(&source, &TickClock(100)).await.unwrap();
    let rows = all_dropped(&pool, profile).await;
    let company_rows: Vec<&DroppedCandidate> =
        rows.iter().filter(|row| row.agent.is_none()).collect();
    assert_eq!(company_rows.len(), 2);

    let known = usecase
        .review_dropped(
            dropped_of(&rows, "known.fr").id,
            DropReview::ShouldHaveKept,
            actor(),
            at(500),
        )
        .await
        .unwrap();
    let unknown = usecase
        .review_dropped(
            dropped_of(&rows, "unknown.fr").id,
            DropReview::ShouldHaveKept,
            actor(),
            at(500),
        )
        .await
        .unwrap();

    assert!(known.lead_id.is_some());
    assert_eq!(signals_of(&pool, profile, "known.fr").await.len(), 1);
    assert_eq!(unknown.lead_id, None);
    assert_eq!(unknown.candidate.review, Some(DropReview::ShouldHaveKept));
    assert!(!has_lead(&pool, profile, "unknown.fr").await);
}

fn stored(
    profile: ProfileId,
    host: &str,
    agent: Option<&str>,
    reason: DropReason,
    dropped_at: DateTime<Utc>,
) -> DroppedCandidate {
    DroppedCandidate {
        id: DroppedId(Uuid::now_v7()),
        profile_id: profile,
        agent: agent.map(AgentId::new),
        source: SourceName::Boamp,
        kind: SignalKind::parse("job_offer").unwrap(),
        domain: domain(host),
        title: "Titre".to_owned(),
        excerpts: vec!["passage".to_owned()],
        provenance: Provenance::parse(&format!("https://jobs.example/{host}")).unwrap(),
        observed_at: dropped_at,
        reason,
        matched: Vec::new(),
        excluded: Vec::new(),
        details: json!({}),
        dropped_at,
        review: None,
        reviewed_at: None,
        reviewed_by: None,
    }
}

async fn record_all(pool: &PgPool, rows: &[DroppedCandidate]) -> u64 {
    with_tx(pool, async |shared| {
        PgDroppedCandidateRepository::new(&shared)
            .record(rows)
            .await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_purge_removes_unreviewed_rows_older_than_thirty_days_only(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let now = at(0) + Duration::days(100);
    let old = now - Duration::days(31);
    let recent = now - Duration::days(29);
    let rows = vec![
        stored(profile, "old.fr", Some("jobs"), DropReason::NoKeyword, old),
        stored(
            profile,
            "kept-old.fr",
            Some("jobs"),
            DropReason::NoKeyword,
            old,
        ),
        stored(
            profile,
            "good-old.fr",
            Some("jobs"),
            DropReason::NoKeyword,
            old,
        ),
        stored(
            profile,
            "recent.fr",
            Some("jobs"),
            DropReason::NoKeyword,
            recent,
        ),
    ];
    assert_eq!(record_all(&pool, &rows).await, 4);
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .review_dropped(rows[1].id, DropReview::RightlyDropped, actor(), now)
        .await
        .unwrap();
    usecase
        .review_dropped(rows[2].id, DropReview::ShouldHaveKept, actor(), now)
        .await
        .unwrap();

    assert_eq!(usecase.purge_dropped(now).await.unwrap(), 1);
    assert_eq!(usecase.purge_dropped(now).await.unwrap(), 0);

    let left = all_dropped(&pool, profile).await;
    let mut hosts: Vec<&str> = left.iter().map(|row| row.domain.as_str()).collect();
    hosts.sort_unstable();
    assert_eq!(hosts, ["good-old.fr", "kept-old.fr", "recent.fr"]);
}

#[test]
fn the_purge_job_is_daily_deduplicated_and_keeps_thirty_days() {
    let job = dropped_purge_job(at(0)).unwrap();
    assert_eq!(job.kind.as_str(), DROPPED_PURGE_JOB_KIND);
    assert_eq!(DROPPED_PURGE_JOB_KIND, "triage.purge_dropped");
    assert_eq!(job.dedup_key.as_deref(), Some("triage:purge_dropped"));
    assert_eq!(next_dropped_purge(at(0)) - at(0), Duration::hours(24));
    assert_eq!(DROPPED_RETENTION_DAYS, 30);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_recall_counts_drops_reviews_and_wrong_drops_per_agent_and_reason(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let other = profile_with(&pool, "other", vec![jobs_agent("jobs")]).await;
    let rows = vec![
        stored(profile, "a.fr", Some("jobs"), DropReason::NoKeyword, at(10)),
        stored(profile, "b.fr", Some("jobs"), DropReason::NoKeyword, at(20)),
        stored(profile, "c.fr", Some("jobs"), DropReason::NoKeyword, at(30)),
        stored(profile, "d.fr", Some("jobs"), DropReason::Exclusion, at(40)),
        stored(
            profile,
            "e.fr",
            Some("second"),
            DropReason::NoKeyword,
            at(50),
        ),
        stored(profile, "f.fr", None, DropReason::FilterRemote, at(60)),
        stored(other, "a.fr", Some("jobs"), DropReason::NoKeyword, at(70)),
    ];
    record_all(&pool, &rows).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .review_dropped(rows[0].id, DropReview::ShouldHaveKept, actor(), at(100))
        .await
        .unwrap();
    usecase
        .review_dropped(rows[1].id, DropReview::RightlyDropped, actor(), at(100))
        .await
        .unwrap();
    usecase
        .review_dropped(rows[3].id, DropReview::RightlyDropped, actor(), at(100))
        .await
        .unwrap();

    let recall = with_tx(&pool, async |shared| {
        PgDroppedCandidateRepository::new(&shared)
            .recall(Some(profile), None, None)
            .await
    })
    .await
    .unwrap();

    let row = |agent: Option<&str>, reason: DropReason| {
        recall
            .iter()
            .find(|row| row.agent.as_ref().map(AgentId::as_str) == agent && row.reason == reason)
            .unwrap_or_else(|| panic!("no recall row for {agent:?} {reason:?}"))
    };
    assert_eq!(recall.len(), 4);
    assert!(recall.iter().all(|row| row.profile_id == profile));
    let no_keyword = row(Some("jobs"), DropReason::NoKeyword);
    assert_eq!(
        (
            no_keyword.dropped,
            no_keyword.reviewed,
            no_keyword.should_have_kept
        ),
        (3, 2, 1)
    );
    assert_eq!(no_keyword.share(), Some(0.5));
    let exclusion = row(Some("jobs"), DropReason::Exclusion);
    assert_eq!(
        (
            exclusion.dropped,
            exclusion.reviewed,
            exclusion.should_have_kept
        ),
        (1, 1, 0)
    );
    assert_eq!(exclusion.share(), Some(0.0));
    let untouched = row(Some("second"), DropReason::NoKeyword);
    assert_eq!(
        (
            untouched.dropped,
            untouched.reviewed,
            untouched.should_have_kept
        ),
        (1, 0, 0)
    );
    assert_eq!(untouched.share(), None);
    let company = row(None, DropReason::FilterRemote);
    assert_eq!((company.dropped, company.reviewed), (1, 0));

    let everything = with_tx(&pool, async |shared| {
        PgDroppedCandidateRepository::new(&shared)
            .recall(None, None, None)
            .await
    })
    .await
    .unwrap();
    assert_eq!(everything.len(), 5);

    let windowed = with_tx(&pool, async |shared| {
        PgDroppedCandidateRepository::new(&shared)
            .recall(Some(profile), Some(at(20)), Some(at(40)))
            .await
    })
    .await
    .unwrap();
    let dropped: u64 = windowed.iter().map(|row| row.dropped).sum();
    assert_eq!(dropped, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_quality_report_carries_the_triage_recall_of_its_window(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    let rows = vec![
        stored(profile, "a.fr", Some("jobs"), DropReason::NoKeyword, at(10)),
        stored(
            profile,
            "b.fr",
            Some("jobs"),
            DropReason::NoKeyword,
            at(500),
        ),
    ];
    record_all(&pool, &rows).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .review_dropped(rows[0].id, DropReview::ShouldHaveKept, actor(), at(600))
        .await
        .unwrap();

    let report = usecase
        .quality_report(None, None, Some(profile))
        .await
        .unwrap();
    assert_eq!(report.triage_recall.len(), 1);
    assert_eq!(report.triage_recall[0].dropped, 2);
    assert_eq!(report.triage_recall[0].reviewed, 1);
    assert_eq!(report.triage_recall[0].should_have_kept, 1);

    let narrowed = usecase
        .quality_report(Some(at(100)), Some(at(900)), Some(profile))
        .await
        .unwrap();
    assert_eq!(narrowed.triage_recall.len(), 1);
    assert_eq!(narrowed.triage_recall[0].dropped, 1);
    assert_eq!(narrowed.triage_recall[0].reviewed, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_profile_deletes_its_dropped_candidates(pool: PgPool) {
    let profile = profile_with(&pool, "jobs", vec![jobs_agent("jobs")]).await;
    record_all(
        &pool,
        &[stored(
            profile,
            "a.fr",
            Some("jobs"),
            DropReason::NoKeyword,
            at(10),
        )],
    )
    .await;

    sqlx::query("DELETE FROM profiles WHERE id = $1")
        .bind(profile.0)
        .execute(&pool)
        .await
        .unwrap();

    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dropped_candidates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);
}
