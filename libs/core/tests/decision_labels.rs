mod support;

use chrono::{Duration, Utc};
use common::CoreError;
use lodestone_core::{
    AssessmentId, DecisionSnapshot, EvidenceComponents, EvidenceStrength, LeadId, LeadStatus,
    LodestoneUseCase, ProfileId, Suggestion, UserId,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{create_profile, seen_lead};
use uuid::Uuid;

async fn user(pool: &PgPool) -> UserId {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    UserId(id)
}

fn evidence() -> EvidenceStrength {
    EvidenceStrength {
        score: 0.62,
        components: EvidenceComponents {
            volume: 0.5,
            diversity: 1.0,
            environment: 0.5,
            strength: 0.4,
        },
        signal_count: 3,
        signal_score: 8.0,
        only_non_production: false,
    }
}

async fn lead(pool: &PgPool, profile: ProfileId, host: &str, status: &str) -> LeadId {
    let lead = seen_lead(pool, profile, host).await;
    sqlx::query("UPDATE leads SET status = $2 WHERE id = $1")
        .bind(lead.id.0)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    lead.id
}

async fn qualify(pool: &PgPool, lead: LeadId, confidence: f64, verdict: &str) {
    sqlx::query(
        "UPDATE leads SET confidence = $2, verdict = $3, qualified_at = now() WHERE id = $1",
    )
    .bind(lead.0)
    .bind(confidence)
    .bind(verdict)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_version, opinion, evidence, verdict_reason)
         VALUES ($1, 1, ARRAY['http_probe'], now(), 1, 0.81, $2, 'weak_evidence')",
    )
    .bind(lead.0)
    .bind(serde_json::to_value(evidence()).unwrap())
    .execute(pool)
    .await
    .unwrap();
    for (agent, status, score) in [("fit", "valid", Some(0.9)), ("market", "invalid", None)] {
        let profile: Uuid = sqlx::query_scalar("SELECT profile_id FROM leads WHERE id = $1")
            .bind(lead.0)
            .fetch_one(pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO qualification_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_hash, status, answers, agent_score, created_at)
             VALUES ($1, $2, $3, 1, $4, 'model-x', 'hash', $5, '{}'::jsonb, $6, now())",
        )
        .bind(Uuid::now_v7())
        .bind(lead.0)
        .bind(profile)
        .bind(agent)
        .bind(status)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn draft(pool: &PgPool, lead: LeadId) -> AssessmentId {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status)
         VALUES ($1, $2, 1, 'summary', 'angle', '[]'::jsonb, 'model-x', 'qualification@1', now(), 'Subject', 'Body', 'pending')",
    )
    .bind(id)
    .bind(lead.0)
    .execute(pool)
    .await
    .unwrap();
    AssessmentId(id)
}

async fn kinds(pool: &PgPool, lead: LeadId) -> Vec<String> {
    sqlx::query_scalar("SELECT kind FROM decision_labels WHERE lead_id = $1 ORDER BY kind")
        .bind(lead.0)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn total(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM decision_labels")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn validating_a_lead_records_the_qualification_at_decision_time(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    qualify(&pool, id, 72.5, "a_verifier").await;

    LodestoneUseCase::new(pool.clone())
        .change_lead_status(id, LeadStatus::Validated, actor)
        .await
        .unwrap();

    let (kind, by, revision, profile_id, snapshot): (String, Option<Uuid>, i32, Uuid, Value) =
        sqlx::query_as(
            "SELECT kind, decided_by, profile_revision, profile_id, snapshot FROM decision_labels WHERE lead_id = $1",
        )
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kind, "lead_validated");
    assert_eq!(by, Some(actor.0));
    assert_eq!(revision, 1);
    assert_eq!(profile_id, profile.0);
    assert_eq!(snapshot["confidence"], json!(72.5));
    assert_eq!(snapshot["opinion"], json!(0.81));
    assert_eq!(snapshot["verdict"], json!("a_verifier"));
    assert_eq!(snapshot["verdict_reason"], json!("weak_evidence"));
    assert_eq!(snapshot["evidence"]["score"], json!(0.62));
    assert_eq!(snapshot["evidence"]["components"]["diversity"], json!(1.0));
    assert_eq!(
        snapshot["agents"]["fit"],
        json!({"status": "valid", "score": 0.9})
    );
    assert_eq!(
        snapshot["agents"]["market"],
        json!({"status": "invalid", "score": null})
    );
    let parsed: DecisionSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(parsed.evidence, Some(evidence()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_records_a_label_and_going_back_to_new_records_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let rejected = lead(&pool, profile, "a.com", "qualified").await;
    let reopened = lead(&pool, profile, "b.com", "qualified").await;
    let usecase = LodestoneUseCase::new(pool.clone());

    usecase
        .change_lead_status(rejected, LeadStatus::Rejected, actor)
        .await
        .unwrap();
    assert_eq!(kinds(&pool, rejected).await, ["lead_rejected"]);

    usecase
        .change_lead_status(rejected, LeadStatus::New, actor)
        .await
        .unwrap();
    usecase
        .change_lead_status(reopened, LeadStatus::New, actor)
        .await
        .unwrap();
    assert_eq!(kinds(&pool, rejected).await, ["lead_rejected"]);
    assert!(kinds(&pool, reopened).await.is_empty());

    let snapshot: Value =
        sqlx::query_scalar("SELECT snapshot FROM decision_labels WHERE lead_id = $1")
            .bind(rejected.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snapshot["confidence"], Value::Null);
    assert_eq!(snapshot["agents"], json!({}));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_transition_records_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "new").await;
    let error = LodestoneUseCase::new(pool.clone())
        .change_lead_status(id, LeadStatus::Validated, actor)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Conflict(_)));
    assert_eq!(total(&pool).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_a_draft_records_the_draft_and_the_lead_decision(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    qualify(&pool, id, 91.0, "fiable").await;
    let assessment = draft(&pool, id).await;

    LodestoneUseCase::new(pool.clone())
        .approve_draft(assessment, actor)
        .await
        .unwrap();

    assert_eq!(kinds(&pool, id).await, ["draft_approved", "lead_validated"]);
    let confidences: Vec<Value> = sqlx::query_scalar(
        "SELECT snapshot -> 'confidence' FROM decision_labels WHERE lead_id = $1",
    )
    .bind(id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(confidences.iter().all(|value| *value == json!(91.0)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_a_draft_records_the_draft_and_the_lead_decision(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    let assessment = draft(&pool, id).await;

    LodestoneUseCase::new(pool.clone())
        .reject_draft(assessment, actor, Some("off target".to_owned()))
        .await
        .unwrap();

    assert_eq!(kinds(&pool, id).await, ["draft_rejected", "lead_rejected"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_conflicting_draft_decision_records_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "validated").await;
    let assessment = draft(&pool, id).await;
    let error = LodestoneUseCase::new(pool.clone())
        .approve_draft(assessment, actor)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Conflict(_)));
    assert_eq!(total(&pool).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_report_filters_by_period_and_profile_and_ignores_suppressed_domains(pool: PgPool) {
    let alpha = create_profile(&pool, "alpha").await;
    let beta = create_profile(&pool, "beta").await;
    let actor = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let old = lead(&pool, alpha, "old.com", "qualified").await;
    let recent = lead(&pool, alpha, "recent.com", "qualified").await;
    let other = lead(&pool, beta, "other.com", "qualified").await;
    let hidden = lead(&pool, alpha, "hidden.com", "qualified").await;
    qualify(&pool, recent, 80.0, "fiable").await;
    for (lead, to) in [
        (old, LeadStatus::Validated),
        (recent, LeadStatus::Rejected),
        (other, LeadStatus::Validated),
        (hidden, LeadStatus::Validated),
    ] {
        usecase.change_lead_status(lead, to, actor).await.unwrap();
    }
    sqlx::query(
        "UPDATE decision_labels SET decided_at = now() - interval '40 days' WHERE lead_id = $1",
    )
    .bind(old.0)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'domain', 'hidden.com', now())")
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();

    let everything = usecase.quality_report(None, None, None).await.unwrap();
    assert_eq!(everything.decisions.validated, 2);
    assert_eq!(everything.decisions.rejected, 1);
    assert_eq!(everything.profiles.len(), 2);

    let month = usecase
        .quality_report(Some(Utc::now() - Duration::days(30)), None, None)
        .await
        .unwrap();
    assert_eq!(month.decisions.validated, 1);
    assert_eq!(month.decisions.rejected, 1);

    let only_alpha = usecase
        .quality_report(None, None, Some(alpha))
        .await
        .unwrap();
    assert_eq!(only_alpha.decisions.validated, 1);
    assert_eq!(only_alpha.decisions.rejected, 1);
    assert_eq!(only_alpha.profiles.len(), 1);
    assert_eq!(only_alpha.profiles[0].name, "alpha");
    assert_eq!(
        only_alpha.profiles[0].suggestion,
        Suggestion::Insufficient {
            labelled: 1,
            required: 20
        }
    );
    let rejected_row = only_alpha
        .verdicts
        .iter()
        .find(|row| row.rejected == 1)
        .unwrap();
    assert_eq!(rejected_row.precision, Some(0.0));
    assert_eq!(only_alpha.agents[0].agent_id, "fit");

    let unknown = usecase
        .quality_report(None, None, Some(ProfileId(Uuid::now_v7())))
        .await
        .unwrap_err();
    assert!(matches!(unknown, CoreError::NotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_discarded_share_counts_qualified_leads_by_verdict(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    for (host, verdict) in [
        ("a.com", "ecarte"),
        ("b.com", "ecarte"),
        ("c.com", "fiable"),
        ("d.com", "a_verifier"),
    ] {
        let id = lead(&pool, profile, host, "qualified").await;
        qualify(&pool, id, 50.0, verdict).await;
    }
    let report = LodestoneUseCase::new(pool.clone())
        .quality_report(None, None, Some(profile))
        .await
        .unwrap();
    assert_eq!(report.discarded.discarded, 2);
    assert_eq!(report.discarded.total, 4);
    assert_eq!(report.discarded.share, Some(0.5));
}
