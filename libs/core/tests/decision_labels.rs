mod support;

use chrono::{Duration, Utc};
use common::CoreError;
use lodestone_core::{
    AssessmentId, CreateProfileCommand, DecisionSnapshot, EvidenceComponents, EvidenceStrength,
    LeadId, LeadStatus, LodestoneUseCase, ProfileId, ProfileName, Rejection, RejectionReason,
    Suggestion, UserId, ValidatedProfile, VersionKind,
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
        independent_signals: 3,
        decayed_signals: 3.0,
        source_count: 1,
        kind_count: 1,
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
            "INSERT INTO qualification_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, answers, agent_score, created_at, provider_name, generation, samples)
             VALUES ($1, $2, $3, 1, $4, 'model-x', 'qualification-agent@hash', $5, '{}'::jsonb, $6, now(), 'local', '{\"temperature\":0.2,\"max_tokens\":512,\"reasoning_effort\":\"low\"}'::jsonb, 1)",
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
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, provider_name, generation, samples)
         VALUES ($1, $2, 1, 'summary', 'angle', '[]'::jsonb, 'big-model', 'aggregator@abc', now(), 'cloud', '{\"temperature\":0.0}'::jsonb, 1)",
    )
    .bind(Uuid::now_v7())
    .bind(id.0)
    .execute(&pool)
    .await
    .unwrap();

    LodestoneUseCase::new(pool.clone())
        .change_lead_status(id, LeadStatus::Validated, actor, None)
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
    let provenance = json!({
        "provider_id": null,
        "provider_name": "local",
        "model": "model-x",
        "prompt_version": "qualification-agent@hash",
        "generation": {
            "temperature": 0.2,
            "max_tokens": 512,
            "frequency_penalty": null,
            "presence_penalty": null,
            "repeat_penalty": null,
            "reasoning_effort": "low"
        },
        "samples": 1
    });
    assert_eq!(
        snapshot["agents"]["fit"],
        json!({"status": "valid", "score": 0.9, "provenance": provenance, "escalated": false})
    );
    assert_eq!(
        snapshot["agents"]["market"],
        json!({"status": "invalid", "score": null, "provenance": provenance, "escalated": false})
    );
    assert_eq!(snapshot["aggregator"]["model"], json!("big-model"));
    assert_eq!(
        snapshot["aggregator"]["prompt_version"],
        json!("aggregator@abc")
    );
    assert_eq!(snapshot["aggregator"]["provider_name"], json!("cloud"));
    assert_eq!(
        snapshot["aggregator"]["generation"]["temperature"],
        json!(0.0)
    );
    assert_eq!(snapshot["aggregator"]["samples"], json!(1));
    let parsed: DecisionSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(parsed.evidence, Some(evidence()));
}

async fn scored(pool: &PgPool, lead: LeadId) {
    let profile: Uuid = sqlx::query_scalar("SELECT profile_id FROM leads WHERE id = $1")
        .bind(lead.0)
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE lead_qualification_state SET verdict_source = 'scoring', indicator_confidence = 44.5, verdict_reason = NULL WHERE lead_id = $1",
    )
    .bind(lead.0)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("UPDATE leads SET llm_score = 68 WHERE id = $1")
        .bind(lead.0)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO scoring_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, criteria, score, verdict, reasons, created_at, provider_name, generation, samples)
         VALUES ($1, $2, $3, 1, 'score-global', 'model-scorer', 'scoring-agent@hash', 'valid', '{}'::jsonb, 68, $4, '[\"Un signal.\"]'::jsonb, now(), 'cloud', '{\"temperature\":0.0}'::jsonb, 3)",
    )
    .bind(Uuid::now_v7())
    .bind(lead.0)
    .bind(profile)
    .bind(json!({
        "choice": "a_verifier",
        "confidence": 0.6,
        "probabilities": {"fiable": 0.3, "a_verifier": 0.6, "ecarte": 0.1}
    }))
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_snapshot_keeps_the_scoring_results_and_the_report_counts_them_per_agent(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    qualify(&pool, id, 60.0, "a_verifier").await;
    scored(&pool, id).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    usecase
        .change_lead_status(id, LeadStatus::Validated, actor, None)
        .await
        .unwrap();

    let snapshot: Value =
        sqlx::query_scalar("SELECT snapshot FROM decision_labels WHERE lead_id = $1")
            .bind(id.0)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(snapshot["llm_score"], json!(68));
    assert_eq!(snapshot["indicator_confidence"], json!(44.5));
    assert_eq!(snapshot["verdict_source"], json!("scoring"));
    assert_eq!(snapshot["confidence"], json!(60.0));
    let agent = &snapshot["scoring_agents"]["score-global"];
    assert_eq!(agent["status"], json!("valid"));
    assert_eq!(agent["score"], json!(68));
    assert_eq!(agent["verdict"], json!("a_verifier"));
    assert_eq!(agent["confidence"], json!(0.6));
    assert_eq!(agent["provenance"]["model"], json!("model-scorer"));
    assert_eq!(
        agent["provenance"]["prompt_version"],
        json!("scoring-agent@hash")
    );
    assert_eq!(agent["provenance"]["samples"], json!(3));
    let parsed: DecisionSnapshot = serde_json::from_value(snapshot).unwrap();
    assert_eq!(parsed.scoring_agents.len(), 1);

    let report = usecase
        .quality_report(None, None, Some(profile))
        .await
        .unwrap();
    let row = report
        .versions
        .iter()
        .find(|row| row.kind == VersionKind::Scoring)
        .unwrap();
    assert_eq!(row.agent_id.as_deref(), Some("score-global"));
    assert_eq!(row.agent_name.as_deref(), Some("Score global"));
    assert_eq!(row.model, "model-scorer");
    assert_eq!(row.prompt_version, "scoring-agent@hash");
    assert_eq!((row.validated, row.rejected), (1, 0));
    assert_eq!(row.precision, Some(1.0));
    assert!(
        report
            .versions
            .iter()
            .filter(|row| row.kind == VersionKind::Qualification)
            .all(|row| row.agent_id.as_deref() == Some("fit"))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_records_a_label_and_going_back_to_new_records_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let rejected = lead(&pool, profile, "a.com", "qualified").await;
    let reopened = lead(&pool, profile, "b.com", "qualified").await;
    let usecase = LodestoneUseCase::new(pool.clone());

    usecase
        .change_lead_status(
            rejected,
            LeadStatus::Rejected,
            actor,
            Some(Rejection::new(RejectionReason::OffTarget, None).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(kinds(&pool, rejected).await, ["lead_rejected"]);
    let stored: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT rejection_reason, rejection_note FROM decision_labels WHERE lead_id = $1",
    )
    .bind(rejected.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, (Some("hors_cible".to_owned()), None));

    usecase
        .change_lead_status(rejected, LeadStatus::New, actor, None)
        .await
        .unwrap();
    usecase
        .change_lead_status(reopened, LeadStatus::New, actor, None)
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
        .change_lead_status(id, LeadStatus::Validated, actor, None)
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
        .reject_draft(
            assessment,
            actor,
            Rejection::new(RejectionReason::Other, Some("off target")).unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(kinds(&pool, id).await, ["draft_rejected", "lead_rejected"]);
    let stored: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT rejection_reason, rejection_note FROM decision_labels WHERE lead_id = $1",
    )
    .bind(id.0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(stored.len(), 2);
    assert!(
        stored
            .iter()
            .all(|row| *row == (Some("autre".to_owned()), Some("off target".to_owned())))
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_a_lead_without_a_reason_is_refused_and_records_nothing(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    let error = LodestoneUseCase::new(pool.clone())
        .change_lead_status(id, LeadStatus::Rejected, actor, None)
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Invalid { .. }));
    assert_eq!(total(&pool).await, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM leads WHERE id = $1")
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "qualified");
}

#[sqlx::test(migrations = "../../migrations")]
async fn validations_and_approvals_carry_no_rejection_reason(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    let assessment = draft(&pool, id).await;
    LodestoneUseCase::new(pool.clone())
        .approve_draft(assessment, actor)
        .await
        .unwrap();
    let reasons: Vec<Option<String>> =
        sqlx::query_scalar("SELECT rejection_reason FROM decision_labels WHERE lead_id = $1")
            .bind(id.0)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(reasons, [None, None]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_report_names_agents_after_the_profile_revisions_and_lists_reasons_and_versions(
    pool: PgPool,
) {
    let mut definition: Value = serde_json::from_str(include_str!(
        "../../../docs/examples/profiles/iam-ferriskey.json"
    ))
    .unwrap();
    definition["qualification_agents"] = json!([{
        "id": "fit",
        "name": "Adéquation",
        "description": "Fit",
        "enabled": true,
        "system_prompt": "PROMPT",
        "questions": [{
            "id": "q",
            "wording": "Pertinent ?",
            "weight": 5,
            "options": [
                {"value": "oui", "label": "Oui", "score": 1.0},
                {"value": "non", "label": "Non", "score": 0.0},
                {"value": "inconnu", "label": "Inconnu", "score": 0.5, "is_unknown": true}
            ]
        }]
    }]);
    let profile = LodestoneUseCase::new(pool.clone())
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("alpha").unwrap(),
            definition: ValidatedProfile::parse(definition).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified").await;
    qualify(&pool, id, 40.0, "a_verifier").await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .change_lead_status(
            id,
            LeadStatus::Rejected,
            actor,
            Some(Rejection::new(RejectionReason::Competitor, None).unwrap()),
        )
        .await
        .unwrap();

    let report = usecase
        .quality_report(None, None, Some(profile))
        .await
        .unwrap();

    assert_eq!(report.agents.len(), 1);
    assert_eq!(report.agents[0].agent_id, "fit");
    assert_eq!(report.agents[0].name, "Adéquation");
    assert_eq!(report.versions[0].agent_name.as_deref(), Some("Adéquation"));
    assert_eq!(report.versions.len(), 1);
    assert_eq!(report.versions[0].model, "model-x");
    assert_eq!(
        report.versions[0].prompt_version,
        "qualification-agent@hash"
    );
    assert_eq!(
        (report.versions[0].validated, report.versions[0].rejected),
        (0, 1)
    );
    assert_eq!(report.profiles[0].reasons.len(), 1);
    assert_eq!(
        report.profiles[0].reasons[0].reason,
        RejectionReason::Competitor
    );
    assert_eq!(report.profiles[0].reasons[0].count, 1);
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
        usecase
            .change_lead_status(
                lead,
                to,
                actor,
                (to == LeadStatus::Rejected)
                    .then(|| Rejection::new(RejectionReason::OffTarget, None).unwrap()),
            )
            .await
            .unwrap();
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
