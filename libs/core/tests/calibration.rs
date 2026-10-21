mod support;

use chrono::{DateTime, Duration, Utc};
use lodestone_core::{
    CALIBRATION_JOB_KIND, CalibrationKind, CalibrationStatus, CreateProfileCommand, LeadId,
    LodestoneUseCase, ProfileId, ProfileName, ValidatedProfile, VerdictSource,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{MINIMAL_PROFILE, at, seen_lead};
use uuid::Uuid;

const SCORING_AGENT: &str = "score-global";
const QUALIFICATION_AGENT: &str = "pertinence";

fn now() -> DateTime<Utc> {
    at(10_000)
}

fn definition(learned: Option<bool>) -> Value {
    definition_per_agent(learned, learned)
}

fn definition_per_agent(scoring: Option<bool>, qualification: Option<bool>) -> Value {
    let value: Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    let mut value = ValidatedProfile::parse(value).unwrap().to_json();
    for (key, learned) in [
        ("scoring_agents", scoring),
        ("qualification_agents", qualification),
    ] {
        if let Some(learned) = learned {
            for agent in value[key].as_array_mut().unwrap() {
                agent["learned_calibration"] = json!(learned);
            }
        }
    }
    value
}

async fn profile(pool: &PgPool, learned: Option<bool>) -> (LodestoneUseCase, ProfileId) {
    profile_per_agent(pool, learned, learned).await
}

async fn profile_per_agent(
    pool: &PgPool,
    scoring: Option<bool>,
    qualification: Option<bool>,
) -> (LodestoneUseCase, ProfileId) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
            definition: ValidatedProfile::parse(definition_per_agent(scoring, qualification))
                .unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    (usecase, id)
}

fn evidence() -> Value {
    json!({
        "score": 0.5,
        "components": {"volume": 0.5, "diversity": 0.5, "environment": 0.5, "strength": 0.5},
        "signal_count": 0,
        "signal_score": 0.0,
        "only_non_production": false,
        "independent_signals": 0,
        "decayed_signals": 0.0,
        "source_count": 0,
        "kind_count": 0
    })
}

fn choice(options: &[(String, f64)], chosen: &str) -> Value {
    let probabilities: serde_json::Map<String, Value> = options
        .iter()
        .map(|(value, _)| {
            (
                value.clone(),
                json!(if value == chosen { 1.0 } else { 0.0 }),
            )
        })
        .collect();
    json!({
        "type": "choice",
        "choice": chosen,
        "evidence": "",
        "confidence": 1.0,
        "probabilities": probabilities
    })
}

fn qualification_answers(pool_definition: &Value, best: bool) -> Value {
    let agent = pool_definition["qualification_agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|agent| agent["id"] == QUALIFICATION_AGENT)
        .unwrap();
    let mut answers = serde_json::Map::new();
    for question in agent["questions"].as_array().unwrap() {
        let options: Vec<(String, f64)> = question["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| {
                (
                    option["value"].as_str().unwrap().to_owned(),
                    option["score"].as_f64().unwrap(),
                )
            })
            .collect();
        let known: Vec<&(String, f64)> = options
            .iter()
            .filter(|(value, _)| value != "inconnu")
            .collect();
        let chosen = if best {
            known
                .iter()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap()
                .0
                .clone()
        } else {
            known
                .iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap()
                .0
                .clone()
        };
        answers.insert(
            question["id"].as_str().unwrap().to_owned(),
            choice(&options, &chosen),
        );
    }
    Value::Object(answers)
}

struct Seed {
    host: String,
    validated: bool,
    probabilities: (f64, f64, f64),
    best_answers: bool,
}

async fn seed(pool: &PgPool, profile: ProfileId, spec: &Seed) -> LeadId {
    let lead = seen_lead(pool, profile, &spec.host).await;
    sqlx::query(
        "UPDATE leads SET status = $2, confidence = 50, verdict = 'a_verifier', qualified_at = now() WHERE id = $1",
    )
    .bind(lead.id.0)
    .bind(if spec.validated { "validated" } else { "rejected" })
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_version, opinion, evidence, verdict_source, scored_at)
         VALUES ($1, 1, ARRAY['http_probe'], now(), 4, 0.5, $2, 'scoring', now())",
    )
    .bind(lead.id.0)
    .bind(evidence())
    .execute(pool)
    .await
    .unwrap();
    let (reliable, to_check, discarded) = spec.probabilities;
    let verdict = json!({
        "choice": "a_verifier",
        "confidence": 0.5,
        "probabilities": {"fiable": reliable, "a_verifier": to_check, "ecarte": discarded}
    });
    sqlx::query(
        "INSERT INTO scoring_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, criteria, score, verdict, reasons, created_at)
         VALUES ($1, $2, $3, 1, $4, 'm', 'scoring-agent@h', 'valid', '{}'::jsonb, 70, $5, '[\"r\"]'::jsonb, now())",
    )
    .bind(Uuid::now_v7())
    .bind(lead.id.0)
    .bind(profile.0)
    .bind(SCORING_AGENT)
    .bind(verdict)
    .execute(pool)
    .await
    .unwrap();
    let answers = qualification_answers(&definition(None), spec.best_answers);
    sqlx::query(
        "INSERT INTO qualification_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, answers, agent_score, created_at, provider_name, generation, samples)
         VALUES ($1, $2, $3, 1, $4, 'm', 'qualification-agent@h', 'valid', $5, 0.5, now(), 'local', '{\"temperature\":0.2,\"max_tokens\":512,\"reasoning_effort\":\"low\"}'::jsonb, 1)",
    )
    .bind(Uuid::now_v7())
    .bind(lead.id.0)
    .bind(profile.0)
    .bind(QUALIFICATION_AGENT)
    .bind(answers)
    .execute(pool)
    .await
    .unwrap();
    label(
        pool,
        lead.id,
        profile,
        if spec.validated {
            "lead_validated"
        } else {
            "lead_rejected"
        },
        0,
    )
    .await;
    lead.id
}

async fn label(pool: &PgPool, lead: LeadId, profile: ProfileId, kind: &str, seconds: i64) {
    sqlx::query(
        "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
         VALUES ($1, $2, $3, 1, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(lead.0)
    .bind(profile.0)
    .bind(kind)
    .bind(at(seconds))
    .bind(json!({
        "confidence": null, "opinion": null, "evidence": null,
        "verdict": null, "verdict_reason": null, "agents": {}
    }))
    .execute(pool)
    .await
    .unwrap();
}

async fn overconfident(pool: &PgPool, profile: ProfileId, high: usize, low: usize) {
    for index in 0..high {
        seed(
            pool,
            profile,
            &Seed {
                host: format!("high{index}.com"),
                validated: index % 2 == 0,
                probabilities: (0.9, 0.0, 0.1),
                best_answers: index % 2 == 0,
            },
        )
        .await;
    }
    for index in 0..low {
        seed(
            pool,
            profile,
            &Seed {
                host: format!("low{index}.com"),
                validated: index % 10 == 0,
                probabilities: (0.1, 0.0, 0.9),
                best_answers: index % 10 == 0,
            },
        )
        .await;
    }
}

async fn rows(pool: &PgPool) -> Vec<(String, String, i32, i32, bool, Value)> {
    sqlx::query_as(
        "SELECT kind, agent_id, n, positives, enabled, points FROM calibrations ORDER BY kind, agent_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn row<'a>(
    rows: &'a [(String, String, i32, i32, bool, Value)],
    kind: &str,
) -> &'a (String, String, i32, i32, bool, Value) {
    rows.iter().find(|row| row.0 == kind).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn fewer_than_fifty_labelled_leads_store_the_counts_and_no_map(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    overconfident(&pool, profile, 20, 29).await;

    usecase.calibrate_profiles(now()).await.unwrap();

    let stored = rows(&pool).await;
    let scoring = row(&stored, "scoring");
    assert_eq!(scoring.1, SCORING_AGENT);
    assert_eq!(scoring.2, 49);
    assert!(!scoring.4);
    assert_eq!(scoring.5, json!([]));
    let report = usecase.calibration_report().await.unwrap();
    let agent = report[0]
        .agents
        .iter()
        .find(|agent| agent.kind == CalibrationKind::Scoring)
        .unwrap();
    assert_eq!(agent.status, CalibrationStatus::Insufficient);
    assert_eq!(agent.n, 49);
    assert!(agent.brier_before.is_some());
    assert!(agent.brier_after.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn ten_of_each_class_among_fifty_leads_activate_the_map(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    overconfident(&pool, profile, 30, 30).await;

    usecase.calibrate_profiles(now()).await.unwrap();

    let stored = rows(&pool).await;
    let scoring = row(&stored, "scoring");
    assert_eq!(scoring.2, 60);
    assert_eq!(scoring.3, 18);
    assert!(scoring.4);
    assert_eq!(scoring.5.as_array().unwrap().len(), 2);
    let qualification = row(&stored, "qualification");
    assert_eq!(qualification.1, QUALIFICATION_AGENT);
    assert_eq!(qualification.2, 60);
    assert!(qualification.4);
    let report = usecase.calibration_report().await.unwrap();
    let agent = report[0]
        .agents
        .iter()
        .find(|agent| agent.kind == CalibrationKind::Scoring)
        .unwrap();
    assert_eq!(agent.status, CalibrationStatus::Active);
    assert!(agent.brier_after.unwrap() < agent.brier_before.unwrap());
    assert!(!agent.reliability.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_class_under_ten_leads_keeps_the_map_off(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    for index in 0..60 {
        seed(
            &pool,
            profile,
            &Seed {
                host: format!("lead{index}.com"),
                validated: index < 9,
                probabilities: (0.5, 0.0, 0.5),
                best_answers: true,
            },
        )
        .await;
    }

    usecase.calibrate_profiles(now()).await.unwrap();

    let stored = rows(&pool).await;
    let scoring = row(&stored, "scoring");
    assert_eq!((scoring.2, scoring.3), (60, 9));
    assert!(!scoring.4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_latest_lead_label_wins_and_draft_labels_are_ignored(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    let flipped = seed(
        &pool,
        profile,
        &Seed {
            host: "flip.com".to_owned(),
            validated: false,
            probabilities: (0.5, 0.0, 0.5),
            best_answers: true,
        },
    )
    .await;
    label(&pool, flipped, profile, "lead_validated", 100).await;
    label(&pool, flipped, profile, "draft_rejected", 200).await;
    let draft_only = seed(
        &pool,
        profile,
        &Seed {
            host: "draft.com".to_owned(),
            validated: true,
            probabilities: (0.5, 0.0, 0.5),
            best_answers: true,
        },
    )
    .await;
    sqlx::query("DELETE FROM decision_labels WHERE lead_id = $1")
        .bind(draft_only.0)
        .execute(&pool)
        .await
        .unwrap();
    label(&pool, draft_only, profile, "draft_approved", 0).await;

    usecase.calibrate_profiles(now()).await.unwrap();

    let stored = rows(&pool).await;
    let scoring = row(&stored, "scoring");
    assert_eq!((scoring.2, scoring.3), (1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn refitting_twice_is_idempotent_and_marks_the_scorings_stale_only_when_the_map_moves(
    pool: PgPool,
) {
    let (usecase, profile) = profile(&pool, None).await;
    overconfident(&pool, profile, 30, 30).await;

    let first = usecase.calibrate_profiles(now()).await.unwrap();
    assert!(first.rescored > 0);
    let after_first = rows(&pool).await;
    let stale: i64 =
        sqlx::query_scalar("SELECT count(*) FROM lead_qualification_state WHERE scored_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stale, 60);
    sqlx::query("UPDATE lead_qualification_state SET scored_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    let second = usecase
        .calibrate_profiles(now() + Duration::days(1))
        .await
        .unwrap();

    assert_eq!(second.rescored, 0);
    assert_eq!(rows(&pool).await, after_first);
    let still_fresh: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM lead_qualification_state WHERE scored_at IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(still_fresh, 60);
    let rows_count: i64 = sqlx::query_scalar("SELECT count(*) FROM calibrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows_count, 2);

    seed(
        &pool,
        profile,
        &Seed {
            host: "extra.com".to_owned(),
            validated: true,
            probabilities: (0.1, 0.0, 0.9),
            best_answers: true,
        },
    )
    .await;
    let third = usecase
        .calibrate_profiles(now() + Duration::days(2))
        .await
        .unwrap();
    assert!(third.rescored > 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rescoring_recombines_the_stored_results_with_the_calibration(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    overconfident(&pool, profile, 30, 30).await;
    let probe = lead_by_host(&pool, profile, "high0.com").await;
    usecase.calibrate_profiles(now()).await.unwrap();

    let report = usecase.rescore_stale_leads(500, now()).await.unwrap();

    assert_eq!(report.leads, 60);
    let detail = usecase.lead_detail(probe).await.unwrap();
    let scoring = detail.scoring.unwrap();
    assert_eq!(scoring.verdict_source, VerdictSource::Scoring);
    let probabilities = scoring.probabilities.unwrap();
    assert!((probabilities.reliable - 0.5).abs() < 1e-9);
    assert!(probabilities.to_check.abs() < 1e-9);
    assert!((probabilities.discarded - 0.5).abs() < 1e-9);
    assert!(!detail.calibrations.is_empty());
    let raw = detail.scoring_results[0].verdict.as_ref().unwrap();
    assert!((raw.probabilities.reliable - 0.9).abs() < 1e-9);
    let calibration = detail
        .calibrations
        .scoring(&detail.scoring_results[0].agent_id)
        .unwrap();
    assert_eq!(calibration.n, 60);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_that_does_not_learn_is_fitted_but_never_applied(pool: PgPool) {
    let (usecase, profile) = profile(&pool, Some(false)).await;
    overconfident(&pool, profile, 30, 30).await;
    let probe = lead_by_host(&pool, profile, "high0.com").await;

    let report = usecase.calibrate_profiles(now()).await.unwrap();

    assert_eq!(report.rescored, 0);
    let stored = rows(&pool).await;
    assert_eq!(row(&stored, "scoring").2, 60);
    let fresh: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM lead_qualification_state WHERE scored_at IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fresh, 60);
    sqlx::query("UPDATE lead_qualification_state SET scored_at = NULL")
        .execute(&pool)
        .await
        .unwrap();
    usecase.rescore_stale_leads(500, now()).await.unwrap();
    let detail = usecase.lead_detail(probe).await.unwrap();
    let probabilities = detail.scoring.unwrap().probabilities.unwrap();
    assert!((probabilities.reliable - 0.9).abs() < 1e-9);
    assert!(detail.calibrations.is_empty());
    let summary = usecase.calibration_report().await.unwrap();
    assert!(
        summary[0]
            .agents
            .iter()
            .all(|agent| agent.status == CalibrationStatus::Disabled)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_agent_applies_its_own_map_only_when_its_flag_is_set(pool: PgPool) {
    let (usecase, profile) = profile_per_agent(&pool, Some(false), Some(true)).await;
    overconfident(&pool, profile, 30, 30).await;
    let probe = lead_by_host(&pool, profile, "high0.com").await;

    let report = usecase.calibrate_profiles(now()).await.unwrap();

    assert!(report.rescored > 0);
    let stored = rows(&pool).await;
    assert_eq!(row(&stored, "scoring").2, 60);
    assert_eq!(row(&stored, "qualification").2, 60);
    usecase.rescore_stale_leads(500, now()).await.unwrap();
    let detail = usecase.lead_detail(probe).await.unwrap();
    let probabilities = detail.scoring.unwrap().probabilities.unwrap();
    assert!((probabilities.reliable - 0.9).abs() < 1e-9);
    assert!(
        detail
            .calibrations
            .scoring(&detail.scoring_results[0].agent_id)
            .is_none()
    );
    assert!(
        detail
            .calibrations
            .qualification(&detail.qualification_results[0].agent_id)
            .is_some()
    );
    let summary = usecase.calibration_report().await.unwrap();
    let status_of = |kind: CalibrationKind| {
        summary[0]
            .agents
            .iter()
            .find(|agent| agent.kind == kind)
            .unwrap()
            .status
    };
    assert_eq!(
        status_of(CalibrationKind::Scoring),
        CalibrationStatus::Disabled
    );
    assert_eq!(
        status_of(CalibrationKind::Qualification),
        CalibrationStatus::Active
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_the_maps_of_learning_agents_mark_leads_stale(pool: PgPool) {
    let (usecase, profile) = profile_per_agent(&pool, Some(true), Some(false)).await;
    overconfident(&pool, profile, 30, 30).await;
    let probe = lead_by_host(&pool, profile, "high0.com").await;

    let first = usecase.calibrate_profiles(now()).await.unwrap();

    assert!(first.rescored > 0);
    usecase.rescore_stale_leads(500, now()).await.unwrap();
    let detail = usecase.lead_detail(probe).await.unwrap();
    assert!(
        detail
            .calibrations
            .scoring(&detail.scoring_results[0].agent_id)
            .is_some()
    );
    assert!(
        detail
            .calibrations
            .qualification(&detail.qualification_results[0].agent_id)
            .is_none()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_qualification_opinion_uses_the_learned_map_when_rescoring(pool: PgPool) {
    let (usecase, profile) = profile(&pool, None).await;
    overconfident(&pool, profile, 30, 30).await;
    let probe = lead_by_host(&pool, profile, "high0.com").await;
    usecase.calibrate_profiles(now()).await.unwrap();
    let points: Value = sqlx::query_scalar(
        "SELECT points FROM calibrations WHERE kind = 'qualification' AND agent_id = $1",
    )
    .bind(QUALIFICATION_AGENT)
    .fetch_one(&pool)
    .await
    .unwrap();
    let expected = points.as_array().unwrap().last().unwrap()["y"]
        .as_f64()
        .unwrap();

    usecase.rescore_stale_leads(500, now()).await.unwrap();

    let stored: Option<f64> = sqlx::query_scalar(
        "SELECT agent_score FROM qualification_results WHERE lead_id = $1 AND agent_id = $2",
    )
    .bind(probe.0)
    .bind(QUALIFICATION_AGENT)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((stored.unwrap() - expected.clamp(0.02, 0.98)).abs() < 1e-9);
    let opinion: Option<f64> =
        sqlx::query_scalar("SELECT opinion FROM lead_qualification_state WHERE lead_id = $1")
            .bind(probe.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!((opinion.unwrap() - expected.clamp(0.02, 0.98)).abs() < 1e-9);
}

async fn lead_by_host(pool: &PgPool, profile: ProfileId, host: &str) -> LeadId {
    let id: Uuid = sqlx::query_scalar("SELECT id FROM leads WHERE profile_id = $1 AND domain = $2")
        .bind(profile.0)
        .bind(host)
        .fetch_one(pool)
        .await
        .unwrap();
    LeadId(id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_without_decisions_still_lists_its_agents(pool: PgPool) {
    let (usecase, _) = profile(&pool, None).await;

    let report = usecase.calibrate_profiles(now()).await.unwrap();

    assert_eq!(report.profiles, 1);
    assert_eq!(report.agents_fitted, 0);
    let stored = rows(&pool).await;
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|row| row.2 == 0 && !row.4));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_manual_run_is_enqueued_once_while_one_is_pending(pool: PgPool) {
    let (usecase, _) = profile(&pool, None).await;

    assert!(usecase.enqueue_calibration(now()).await.unwrap());
    assert!(!usecase.enqueue_calibration(now()).await.unwrap());

    let jobs: Vec<(String, Value, Option<String>)> =
        sqlx::query_as("SELECT kind, payload, dedup_key FROM jobs WHERE kind = $1")
            .bind(CALIBRATION_JOB_KIND)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, CALIBRATION_JOB_KIND);
    assert_eq!(jobs[0].1["recurring"], json!(false));
}
