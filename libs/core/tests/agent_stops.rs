mod support;

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, AgentKind, AgentRegistry, CancelReason, Candidate, Clock, CreateProfileCommand,
    JobQueue, JobStatus, LeadRepository, LodestoneUseCase, Observation, ProfileId, ProfileName,
    Provenance, ReviseProfileCommand, RevisionNumber, Run, RunRepository, RunStatus, Signal,
    SignalKind, SignalRepository, Source, SourceError, SourceName, UnitInterval, ValidatedProfile,
    WorkTarget,
    definition::SourceTarget,
    infrastructure::{
        job::postgres::PgJobQueue, lead::postgres::PgLeadRepository, postgres::with_tx,
        run::postgres::PgRunRepository, signal::postgres::PgSignalRepository,
    },
    sources::source_job,
    triage::CandidateSource,
};
use serde_json::{Value, from_str, json};
use sqlx::{PgPool, query, query_as, query_scalar};
use support::{at, domain, lead_id};
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn iam() -> Value {
    from_str(IAM).unwrap()
}

fn without(mut document: Value, section: &str, id: &str) -> Value {
    document[section]
        .as_array_mut()
        .unwrap()
        .retain(|agent| agent["id"] != id);
    document
}

fn disabled(mut document: Value, section: &str, id: &str) -> Value {
    for agent in document[section].as_array_mut().unwrap() {
        if agent["id"] == id {
            agent["enabled"] = json!(false);
        }
    }
    document
}

async fn active(usecase: &LodestoneUseCase, name: &str, definition: Value) -> ProfileId {
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
    usecase.change_profile_activation(id, true).await.unwrap();
    id
}

async fn revise(usecase: &LodestoneUseCase, id: ProfileId, definition: Value) {
    let revised = usecase
        .update_profile(ReviseProfileCommand {
            profile_id: id,
            definition: ValidatedProfile::parse(definition).unwrap(),
            created_by: None,
        })
        .await
        .unwrap();
    assert!(revised.is_created());
}

async fn runs(pool: &PgPool) -> Vec<Run> {
    with_tx(pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(20).await
    })
    .await
    .unwrap()
}

async fn signals(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
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

struct RemovingSource {
    usecase: LodestoneUseCase,
    hosts: Vec<&'static str>,
    remove_at: &'static str,
}

impl Source for RemovingSource {
    type Item = &'static str;

    fn name(&self) -> SourceName {
        SourceName::DnsCname
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Self::Item>, SourceError> {
        Ok(self.hosts.clone())
    }

    fn evaluate(
        &self,
        item: &Self::Item,
        profile: &ActiveProfile,
        _target: &SourceTarget<'_>,
    ) -> Vec<Observation> {
        if *item == self.remove_at {
            let mut removed = profile.clone();
            removed.revision.number = profile.revision.number.next().unwrap();
            removed.revision.definition =
                ValidatedProfile::parse(without(iam(), "agents", "dns")).unwrap();
            self.usecase.agent_registry().publish(&removed);
        }
        vec![Observation {
            domain: domain(item),
            kind: SignalKind::parse("product_detected").unwrap(),
            value: Some("okta".to_owned()),
            raw: json!({ "host": item }),
            provenance: Provenance::parse(&format!("https://{item}/")).unwrap(),
            strength: UnitInterval::new(0.8).unwrap(),
            observed_at: at(50),
            agent: None,
        }]
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn removing_an_agent_during_a_source_run_stops_after_the_current_item(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let source = RemovingSource {
        usecase: usecase.clone(),
        hosts: vec!["sso.first.com", "sso.second.com", "sso.third.com"],
        remove_at: "sso.second.com",
    };

    let report = usecase.run_source(&source, &FixedClock).await.unwrap();

    assert_eq!(report.cancelled, 1);
    assert_eq!(signals(&pool, profile, "first.com").await.len(), 1);
    assert!(signals(&pool, profile, "second.com").await.is_empty());
    assert!(signals(&pool, profile, "third.com").await.is_empty());
    let runs = runs(&pool).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Cancelled);
    assert_eq!(runs[0].cancel_reason, Some(CancelReason::AgentRemoved));
    assert_eq!(runs[0].volume, 1);
    assert_eq!(runs[0].signals, 1);
    assert!(runs[0].finished_at.is_some());
}

struct RevisingSource {
    usecase: LodestoneUseCase,
    profile: ProfileId,
    definition: Value,
}

fn tender(host: &str) -> Candidate {
    Candidate {
        domain: domain(host),
        source: SourceName::Boamp,
        kind: SignalKind::parse("public_tender").unwrap(),
        title: "Mise en place d'un SSO avec MFA".to_owned(),
        text: "Projet IAM, Keycloak et SSO".to_owned(),
        provenance: Provenance::parse(&format!("https://marches.example/avis/{host}")).unwrap(),
        observed_at: at(50),
        details: json!({"idweb": host}),
    }
}

impl CandidateSource for RevisingSource {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        revise(&self.usecase, self.profile, self.definition.clone()).await;
        Ok(vec![tender("mairie.fr")])
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_shared_source_run_continues_for_the_agents_still_live(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let removed = active(&usecase, "removed", iam()).await;
    let kept = active(&usecase, "kept", iam()).await;
    let source = RevisingSource {
        usecase: usecase.clone(),
        profile: removed,
        definition: without(iam(), "agents", "marches-publics"),
    };

    let report = usecase.run_triage(&source, &FixedClock).await.unwrap();

    assert_eq!(report.runs, 2);
    assert_eq!(report.cancelled, 1);
    assert_eq!(report.all_cancelled(), None);
    assert!(signals(&pool, removed, "mairie.fr").await.is_empty());
    assert_eq!(signals(&pool, kept, "mairie.fr").await.len(), 1);
    let runs = runs(&pool).await;
    let of = |profile: ProfileId| {
        runs.iter()
            .find(|run| run.profile_id == profile)
            .unwrap()
            .clone()
    };
    assert_eq!(of(removed).status, RunStatus::Cancelled);
    assert_eq!(of(removed).cancel_reason, Some(CancelReason::AgentRemoved));
    assert_eq!(of(removed).signals, 0);
    assert_eq!(of(kept).status, RunStatus::Succeeded);
    assert_eq!(of(kept).signals, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_source_run_with_every_agent_stopped_reports_it_for_its_job(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let source = RevisingSource {
        usecase: usecase.clone(),
        profile,
        definition: disabled(iam(), "agents", "marches-publics"),
    };

    let report = usecase.run_triage(&source, &FixedClock).await.unwrap();

    assert_eq!(report.all_cancelled(), Some(CancelReason::AgentDisabled));
    assert!(!usecase.has_source_targets(SourceName::Boamp).await.unwrap());
    assert!(signals(&pool, profile, "mairie.fr").await.is_empty());
}

async fn job_status(pool: &PgPool, source: SourceName) -> Vec<(String, Option<String>)> {
    query_as("SELECT status, last_error FROM jobs WHERE dedup_key = $1 ORDER BY created_at, id")
        .bind(format!("source:{}", source.as_str()))
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_queued_job_serving_only_removed_agents_is_cancelled_and_restored_later(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let queue = PgJobQueue::new(pool.clone());
    for source in [SourceName::Boamp, SourceName::FranceTravail] {
        queue
            .enqueue(&source_job(source, at(0)).unwrap(), at(0))
            .await
            .unwrap()
            .unwrap();
    }

    revise(
        &usecase,
        profile,
        without(iam(), "agents", "marches-publics"),
    )
    .await;

    assert_eq!(
        job_status(&pool, SourceName::Boamp).await,
        vec![(
            JobStatus::Cancelled.as_str().to_owned(),
            Some("agent_removed".to_owned())
        )]
    );
    assert_eq!(
        job_status(&pool, SourceName::FranceTravail).await,
        vec![(JobStatus::Pending.as_str().to_owned(), None)]
    );

    revise(&usecase, profile, iam()).await;

    let boamp = job_status(&pool, SourceName::Boamp).await;
    assert_eq!(boamp.len(), 2);
    assert!(
        boamp
            .iter()
            .any(|(status, _)| status == JobStatus::Pending.as_str())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_shared_queued_job_keeps_running_while_another_profile_needs_it(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    active(&usecase, "other", iam()).await;
    PgJobQueue::new(pool.clone())
        .enqueue(&source_job(SourceName::Boamp, at(0)).unwrap(), at(0))
        .await
        .unwrap()
        .unwrap();

    usecase
        .change_profile_activation(profile, false)
        .await
        .unwrap();

    assert_eq!(
        job_status(&pool, SourceName::Boamp).await,
        vec![(JobStatus::Pending.as_str().to_owned(), None)]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivating_the_last_profile_cancels_its_queued_jobs(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    PgJobQueue::new(pool.clone())
        .enqueue(&source_job(SourceName::Ted, at(0)).unwrap(), at(0))
        .await
        .unwrap()
        .unwrap();

    usecase
        .change_profile_activation(profile, false)
        .await
        .unwrap();

    assert_eq!(
        job_status(&pool, SourceName::Ted).await,
        vec![(
            JobStatus::Cancelled.as_str().to_owned(),
            Some("profile_deactivated".to_owned())
        )]
    );
}

async fn add_retry(pool: &PgPool, lead: Uuid, kind: &str, agent: &str) {
    query(
        "INSERT INTO lead_agent_retries (lead_id, kind, profile_revision, agent_id, attempts, next_attempt_at, error, updated_at) VALUES ($1, $2, 1, $3, 2, $4, 'boom', $4)",
    )
    .bind(lead)
    .bind(kind)
    .bind(agent)
    .bind(at(0))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_retries_of_a_removed_agent_are_deleted_and_the_others_kept(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let lead = lead_id(&pool, profile, "acme.com").await;
    add_retry(&pool, lead.0, "qualification", "marche").await;
    add_retry(&pool, lead.0, "scoring", "score-global").await;

    revise(
        &usecase,
        profile,
        without(iam(), "qualification_agents", "marche"),
    )
    .await;

    let failures = usecase.lead_agent_failures(lead).await.unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, AgentKind::Scoring);
    assert_eq!(failures[0].agent_id, "score-global");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivating_a_profile_clears_every_retry_of_its_leads(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let lead = lead_id(&pool, profile, "acme.com").await;
    add_retry(&pool, lead.0, "qualification", "marche").await;

    usecase
        .change_profile_activation(profile, false)
        .await
        .unwrap();

    assert!(usecase.lead_agent_failures(lead).await.unwrap().is_empty());
}

async fn add_result(pool: &PgPool, table: &str, lead: Uuid, profile: ProfileId, agent: &str) {
    query(&format!(
        "INSERT INTO {table} (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, error, created_at) VALUES ($1, $2, $3, 1, $4, 'm', 'v', 'invalid', 'x', $5)"
    ))
    .bind(Uuid::now_v7())
    .bind(lead)
    .bind(profile.0)
    .bind(agent)
    .bind(at(0))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn removing_an_agent_only_carries_the_other_results_to_the_new_revision(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let lead = lead_id(&pool, profile, "acme.com").await;
    query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_revision, scoring_for, scored_at) VALUES ($1, 1, ARRAY['boamp'], $2, 1, $2, $2)",
    )
    .bind(lead.0)
    .bind(at(0))
    .execute(&pool)
    .await
    .unwrap();
    add_result(&pool, "qualification_results", lead.0, profile, "marche").await;
    add_result(
        &pool,
        "qualification_results",
        lead.0,
        profile,
        "pertinence",
    )
    .await;
    add_result(&pool, "scoring_results", lead.0, profile, "score-global").await;

    revise(
        &usecase,
        profile,
        without(iam(), "qualification_agents", "marche"),
    )
    .await;

    let results = usecase.lead_qualification_results(lead).await.unwrap();
    assert_eq!(
        results
            .iter()
            .map(|result| result.agent_id.as_str())
            .collect::<Vec<_>>(),
        vec!["pertinence"]
    );
    assert_eq!(results[0].profile_revision, RevisionNumber::new(2).unwrap());
    assert_eq!(usecase.lead_scoring_results(lead).await.unwrap().len(), 1);
    let (revision, scoring_revision, scored_at): (i32, Option<i32>, Option<DateTime<Utc>>) =
        query_as(
            "SELECT profile_revision, scoring_revision, scored_at FROM lead_qualification_state WHERE lead_id = $1",
        )
        .bind(lead.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((revision, scoring_revision, scored_at), (2, Some(2), None));
}

#[sqlx::test(migrations = "../../migrations")]
async fn any_other_edit_leaves_the_results_on_their_revision(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let lead = lead_id(&pool, profile, "acme.com").await;
    query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at) VALUES ($1, 1, ARRAY['boamp'], $2)",
    )
    .bind(lead.0)
    .bind(at(0))
    .execute(&pool)
    .await
    .unwrap();
    add_result(&pool, "qualification_results", lead.0, profile, "marche").await;
    let mut edited = without(iam(), "qualification_agents", "marche");
    edited["identity"]["offer"] = json!("Autre offre");

    revise(&usecase, profile, edited).await;

    let revision: i32 =
        query_scalar("SELECT profile_revision FROM lead_qualification_state WHERE lead_id = $1")
            .bind(lead.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revision, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_deleted_profile_stops_its_tokens_and_disappears(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, "iam", iam()).await;
    let current = usecase
        .refreshed_active_profiles()
        .await
        .unwrap()
        .into_iter()
        .find(|active| active.profile.id == profile)
        .unwrap();
    let work = usecase.agent_work(&current, WorkTarget::aggregator());
    assert!(work.is_live());

    usecase.delete_profile(profile).await.unwrap();

    assert_eq!(work.stop_reason(), Some(CancelReason::ProfileDeleted));
    assert!(usecase.list_profiles().await.unwrap().is_empty());
    assert!(usecase.delete_profile(profile).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn another_instance_sees_a_change_at_its_next_pass(pool: PgPool) {
    let editor = LodestoneUseCase::new(pool.clone());
    let worker = LodestoneUseCase::new(pool.clone());
    let profile = active(&editor, "iam", iam()).await;
    let current = worker
        .refreshed_active_profiles()
        .await
        .unwrap()
        .into_iter()
        .find(|active| active.profile.id == profile)
        .unwrap();
    let work = worker.agent_work(&current, WorkTarget::aggregator());

    editor
        .change_profile_activation(profile, false)
        .await
        .unwrap();
    assert!(work.is_live());
    worker.refreshed_active_profiles().await.unwrap();

    assert_eq!(work.stop_reason(), Some(CancelReason::ProfileDeactivated));
}
