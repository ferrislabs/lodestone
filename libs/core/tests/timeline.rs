mod support;

use chrono::Duration;
use common::generate_uuid_v7;
use lodestone_core::{
    AgentStop, CancelReason, EventActor, EventStatus, LeadId, LodestoneUseCase, Observation,
    ProfileId, Provenance, RECONSTRUCTED, ROUND_STARTED, RevisionNumber, RoundTrigger,
    SAAS_TENANT_REASON, Signal, SignalId, SignalKind, SignalRepository, SourceName, Stage,
    StageReason, StageStatus, TimelineEntry, TimelineEvent, TimelineRepository, TimelineStep,
    UnitInterval, WorkKind, WorkTarget,
    definition::AgentId,
    infrastructure::{
        postgres::with_tx, signal::postgres::PgSignalRepository,
        timeline::postgres::PgTimelineRepository,
    },
    timeline::{TIMELINE_PURGE_JOB_KIND, next_timeline_purge, timeline_purge_job},
    timeline_kind,
};
use serde_json::json;
use sqlx::{PgPool, query, query_as, query_scalar};
use support::{at, create_profile, domain, lead_id, seen_lead};
use uuid::Uuid;

async fn record(pool: &PgPool, lead: LeadId, entry: &TimelineEntry) -> bool {
    with_tx(pool, async |shared| {
        PgTimelineRepository::new(&shared).record(lead, entry).await
    })
    .await
    .unwrap()
}

fn signal_entry(key: &str, minute: i64) -> TimelineEntry {
    TimelineEntry::new(
        TimelineStep::Detection,
        timeline_kind::SIGNAL,
        EventStatus::Succeeded,
        at(minute * 60),
        key.to_owned(),
    )
    .actor(EventActor::source("http_probe"))
}

async fn rounds_of(pool: &PgPool, lead: LeadId) -> Vec<(i32, String)> {
    query_as::<_, (i32, String)>(
        "SELECT round, dedupe_key FROM lead_events WHERE lead_id = $1 ORDER BY started_at, dedupe_key",
    )
    .bind(lead.0)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn recording_the_same_event_twice_keeps_one(pool: PgPool) {
    let profile = create_profile(&pool, "iam").await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    let entry = signal_entry("signal:abc", 1);
    assert!(record(&pool, lead, &entry).await);
    assert!(!record(&pool, lead, &entry).await);
    let count: i64 = query_scalar("SELECT COUNT(*) FROM lead_events WHERE lead_id = $1")
        .bind(lead.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_lead_records_nothing(pool: PgPool) {
    let entry = signal_entry("signal:abc", 1);
    assert!(!record(&pool, LeadId(Uuid::nil()), &entry).await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn inputs_feed_the_next_round_and_work_stays_in_the_current_one(pool: PgPool) {
    let profile = create_profile(&pool, "iam").await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    record(&pool, lead, &signal_entry("signal:1", 1)).await;
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(at(120), "first", None),
    )
    .await;
    let agent = TimelineEntry::ended(
        TimelineStep::Qualification,
        timeline_kind::AGENT,
        EventStatus::Succeeded,
        ("agent:stack", at(130)),
        at(140),
    );
    record(&pool, lead, &agent).await;
    record(&pool, lead, &signal_entry("signal:2", 3)).await;
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(at(240), "requalification", Some(at(140))),
    )
    .await;
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(at(250), "requalification", Some(at(140))),
    )
    .await;
    let rounds: Vec<i32> = rounds_of(&pool, lead)
        .await
        .into_iter()
        .map(|(round, _)| round)
        .collect();
    assert_eq!(rounds, vec![1, 1, 1, 2, 2]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_timeline_orders_events_by_time_whatever_their_arrival(pool: PgPool) {
    let profile = create_profile(&pool, "iam").await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    for (key, minute) in [("signal:c", 9), ("signal:a", 1), ("signal:b", 5)] {
        record(&pool, lead, &signal_entry(key, minute)).await;
    }
    let timeline = usecase.lead_timeline(lead).await.unwrap();
    assert_eq!(timeline.rounds.len(), 1);
    let keys: Vec<String> = timeline.rounds[0]
        .events
        .iter()
        .map(|event| event.dedupe_key.rsplit(':').next().unwrap().to_owned())
        .collect();
    assert_eq!(keys, vec!["c", "b", "a"]);
    assert_eq!(timeline.rounds[0].trigger, RoundTrigger::Pending);
    assert_eq!(timeline.pipeline.stages[0].stage, Stage::Detection);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_domain_event_reaches_every_lead_of_the_domain(pool: PgPool) {
    let iam = create_profile(&pool, "iam").await;
    let other = create_profile(&pool, "other").await;
    let first = lead_id(&pool, iam, "acme.fr").await;
    let second = lead_id(&pool, other, "acme.fr").await;
    let unrelated = lead_id(&pool, iam, "globex.fr").await;
    let entry = TimelineEntry::new(
        TimelineStep::Inspection,
        timeline_kind::MODULE,
        EventStatus::Succeeded,
        at(60),
        "module:website_stack:1".to_owned(),
    )
    .actor(EventActor::module("website_stack"));
    let recorded = with_tx(&pool, async |shared| {
        let mut repository = PgTimelineRepository::new(&shared);
        let first_pass = repository
            .record_for_domain(&domain("acme.fr"), &entry)
            .await?;
        let second_pass = repository
            .record_for_domain(&domain("acme.fr"), &entry)
            .await?;
        Ok((first_pass, second_pass))
    })
    .await
    .unwrap();
    assert_eq!(recorded, (2, 0));
    for lead in [first, second] {
        assert_eq!(rounds_of(&pool, lead).await.len(), 1);
    }
    assert!(rounds_of(&pool, unrelated).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_retention_purge_keeps_the_round_counter(pool: PgPool) {
    let profile = create_profile(&pool, "iam").await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    record(&pool, lead, &signal_entry("signal:old", 0)).await;
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(at(10), "first", None),
    )
    .await;
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(at(20), "requalification", Some(at(15))),
    )
    .await;
    let now = at(0) + Duration::days(91);
    record(&pool, lead, &signal_entry("signal:fresh", 91 * 24 * 60 - 5)).await;
    assert_eq!(usecase.purge_lead_events(now, 90).await.unwrap(), 2);
    assert_eq!(usecase.purge_lead_events(now, 90).await.unwrap(), 0);
    let left = rounds_of(&pool, lead).await;
    assert_eq!(left.len(), 2);
    assert!(
        left.iter()
            .any(|(round, key)| *round == 2 && key.contains("round:"))
    );
    record(
        &pool,
        lead,
        &TimelineEntry::round_started(now, "requalification", Some(at(30))),
    )
    .await;
    let latest: i32 = query_scalar(
        "SELECT MAX(round) FROM lead_events WHERE lead_id = $1 AND kind = 'round_started'",
    )
    .bind(lead.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(latest, 3);
}

#[test]
fn the_retention_job_is_daily_and_deduplicated() {
    let job = timeline_purge_job(at(0)).unwrap();
    assert_eq!(job.kind.as_str(), TIMELINE_PURGE_JOB_KIND);
    assert_eq!(job.dedup_key.as_deref(), Some("timeline:purge"));
    assert_eq!(next_timeline_purge(at(0)) - at(0), Duration::hours(24));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_backfill_reconstructs_existing_leads_once(pool: PgPool) {
    let profile = create_profile(&pool, "iam").await;
    let old = lead_id(&pool, profile, "acme.fr").await;
    let live = lead_id(&pool, profile, "globex.fr").await;
    with_tx(&pool, async |shared| {
        PgSignalRepository::new(&shared)
            .record(&Signal {
                id: SignalId(generate_uuid_v7()),
                lead_id: old,
                kind: SignalKind::parse("product_detected").unwrap(),
                value: Some("keycloak".to_owned()),
                source: SourceName::HttpProbe,
                raw: json!({}),
                provenance: Provenance::parse("https://acme.fr/realms/master").unwrap(),
                strength: UnitInterval::new(0.5).unwrap(),
                observed_at: at(5),
                agent: None,
            })
            .await
    })
    .await
    .unwrap();
    query("INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at) VALUES ($1, 1, ARRAY['http_probe'], $2)")
        .bind(old.0)
        .bind(at(50))
        .execute(&pool)
        .await
        .unwrap();
    query("INSERT INTO lead_agent_retries (lead_id, kind, profile_revision, agent_id, attempts, next_attempt_at, error, updated_at) VALUES ($1, 'scoring', 1, 'fit', 1, $2, 'timeout', $3)")
        .bind(old.0)
        .bind(at(900))
        .bind(at(60))
        .execute(&pool)
        .await
        .unwrap();
    record(&pool, live, &signal_entry("signal:live", 1)).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    assert_eq!(usecase.backfill_lead_events().await.unwrap(), 3);
    assert_eq!(usecase.backfill_lead_events().await.unwrap(), 0);
    let timeline = usecase.lead_timeline(old).await.unwrap();
    let events: Vec<_> = timeline
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .collect();
    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|event| event.detail[RECONSTRUCTED] == json!(true))
    );
    assert!(timeline.rounds[0].reconstructed);
    assert!(events.iter().any(|event| event.kind == ROUND_STARTED));
    assert!(
        events.iter().any(
            |event| event.step == TimelineStep::Scoring && event.status == EventStatus::Waiting
        )
    );
    assert_eq!(rounds_of(&pool, live).await.len(), 1);
    assert_eq!(timeline.pipeline.stages[0].status, StageStatus::Done);
}

async fn active(usecase: &LodestoneUseCase, pool: &PgPool) -> ProfileId {
    let profile = create_profile(pool, "iam").await;
    usecase.set_profile_active(profile, true).await.unwrap();
    profile
}

async fn events(usecase: &LodestoneUseCase, lead: LeadId) -> Vec<TimelineEvent> {
    usecase
        .lead_timeline(lead)
        .await
        .unwrap()
        .rounds
        .into_iter()
        .flat_map(|round| round.events)
        .collect()
}

fn observation(host: &str, value: &str) -> Observation {
    Observation {
        domain: domain(host),
        kind: SignalKind::parse("product_detected").unwrap(),
        value: Some(value.to_owned()),
        raw: json!({}),
        provenance: Provenance::parse(&format!("https://{host}/{value}")).unwrap(),
        strength: UnitInterval::new(0.5).unwrap(),
        observed_at: at(5),
        agent: Some(AgentId::new("probe")),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_new_signal_is_detected_once(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, &pool).await;
    let current = usecase.list_active_profiles().await.unwrap().remove(0);
    let seen = [observation("acme.fr", "keycloak")];
    usecase
        .record_observations_at(&current, SourceName::HttpProbe, &seen, at(10))
        .await
        .unwrap();
    usecase
        .record_observations_at(&current, SourceName::HttpProbe, &seen, at(20))
        .await
        .unwrap();
    usecase
        .record_observations_at(
            &current,
            SourceName::HttpProbe,
            &[observation("acme.fr", "okta")],
            at(30),
        )
        .await
        .unwrap();
    let lead = lead_id(&pool, profile, "acme.fr").await;
    let detected = events(&usecase, lead).await;
    assert_eq!(detected.len(), 2);
    assert!(
        detected
            .iter()
            .all(|event| event.step == TimelineStep::Detection)
    );
    assert_eq!(detected[0].detail["value"], "okta");
    assert_eq!(detected[0].actor.source.as_deref(), Some("http_probe"));
    assert_eq!(detected[0].actor.agent_id.as_deref(), Some("probe"));
    assert_eq!(detected[0].round, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_excluded_lead_records_its_exclusion_and_blocks_the_pipeline(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, &pool).await;
    let lead = seen_lead(&pool, profile, "force.com").await;
    assert_eq!(usecase.exclude_saas_tenant_leads(at(50)).await.unwrap(), 1);
    assert_eq!(usecase.exclude_saas_tenant_leads(at(60)).await.unwrap(), 0);
    let timeline = usecase.lead_timeline(lead.id).await.unwrap();
    let recorded: Vec<&TimelineEvent> = timeline
        .rounds
        .iter()
        .flat_map(|round| round.events.iter())
        .collect();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].step, TimelineStep::Exclusion);
    assert_eq!(recorded[0].detail["reason"], SAAS_TENANT_REASON);
    let inspection = &timeline.pipeline.stages[1];
    assert_eq!(inspection.stage, Stage::Inspection);
    assert_eq!(inspection.status, StageStatus::Blocked);
    assert_eq!(inspection.reason_code, Some(StageReason::Excluded));
    assert!(
        timeline.pipeline.stages[2..]
            .iter()
            .all(|state| state.status == StageStatus::Skipped)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stopped_agent_is_recorded_once(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, &pool).await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    let stop = AgentStop {
        lead_id: lead,
        target: WorkTarget::new(WorkKind::Scoring, &AgentId::new("fit")),
        revision: RevisionNumber::new(1).unwrap(),
        reason: CancelReason::AgentDisabled,
        stopped_at: at(40),
    };
    usecase.record_agent_stop(&stop).await.unwrap();
    usecase.record_agent_stop(&stop).await.unwrap();
    let stopped = events(&usecase, lead).await;
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].step, TimelineStep::Stop);
    assert_eq!(stopped[0].detail["reason"], "agent_disabled");
    assert_eq!(stopped[0].actor.agent_id.as_deref(), Some("fit"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_manual_reevaluation_is_recorded_for_the_next_round(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let profile = active(&usecase, &pool).await;
    let lead = lead_id(&pool, profile, "acme.fr").await;
    usecase
        .request_lead_reevaluation_at(lead, at(70))
        .await
        .unwrap();
    let requested = events(&usecase, lead).await;
    assert_eq!(requested.len(), 1);
    assert_eq!(requested[0].step, TimelineStep::Requalification);
    assert_eq!(requested[0].kind, "reevaluation_requested");
    let timeline = usecase.lead_timeline(lead).await.unwrap();
    assert_eq!(timeline.rounds[0].trigger, RoundTrigger::Manual);
}
