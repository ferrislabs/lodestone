mod support;

use std::num::NonZeroU16;

use lodestone_core::output::RecordLookup;
use lodestone_core::{
    LeadId, LeadQuery, LeadStatus, LodestoneUseCase, ProfileId, UserId, WebhookPolicy,
};
use sqlx::PgPool;
use support::{create_profile, seen_lead};
use uuid::Uuid;

async fn lead(pool: &PgPool, profile: ProfileId, host: &str, status: &str, score: f64) -> LeadId {
    let lead = seen_lead(pool, profile, host).await;
    sqlx::query("UPDATE leads SET status = $2, score = $3 WHERE id = $1")
        .bind(lead.id.0)
        .bind(status)
        .bind(score)
        .execute(pool)
        .await
        .unwrap();
    lead.id
}

async fn assessment(pool: &PgPool, lead: LeadId) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status)
         VALUES ($1, $2, 1, 'summary', 'angle', '[{\"quote\":\"q\",\"provenance\":\"https://a.com/x\"}]'::jsonb, 'model-x', 'qualification@1', now(), 'Subject', 'Body', 'pending')",
    )
    .bind(id)
    .bind(lead.0)
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn contact(pool: &PgPool, lead: LeadId, address: &str, confidence: f64) {
    sqlx::query(
        "INSERT INTO contact_emails (id, lead_id, address, provenance, method, confidence, kind, found_at)
         VALUES ($1, $2, $3, 'https://a.com/contact', 'public_page', $4, 'role', now())",
    )
    .bind(Uuid::now_v7())
    .bind(lead.0)
    .bind(address)
    .bind(confidence)
    .execute(pool)
    .await
    .unwrap();
}

async fn suppress(pool: &PgPool, kind: &str, value: &str) {
    sqlx::query(
        "INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, $2, $3, now())",
    )
    .bind(Uuid::now_v7())
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn user(pool: &PgPool) -> UserId {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    UserId(id)
}

async fn webhook_jobs(pool: &PgPool) -> Vec<serde_json::Value> {
    sqlx::query_scalar(
        "SELECT payload FROM jobs WHERE kind = 'webhook.deliver' ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn policy(ids: Vec<ProfileId>) -> WebhookPolicy {
    WebhookPolicy {
        profile_ids: ids,
        max_attempts: NonZeroU16::new(5).unwrap(),
    }
}

fn query() -> LeadQuery {
    LeadQuery {
        limit: 20,
        ..LeadQuery::default()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn records_are_scoped_to_the_profile_and_carry_its_identity(pool: PgPool) {
    let alpha = create_profile(&pool, "alpha").await;
    let beta = create_profile(&pool, "beta").await;
    let kept = lead(&pool, alpha, "kept.com", "validated", 9.0).await;
    contact(&pool, kept, "security@kept.com", 0.9).await;
    assessment(&pool, kept).await;
    lead(&pool, beta, "other.com", "validated", 9.0).await;

    let set = LodestoneUseCase::new(pool.clone())
        .lead_records(alpha, query())
        .await
        .unwrap();

    assert_eq!(set.profile.id, alpha.0);
    assert_eq!(set.profile.name, "alpha");
    assert_eq!(set.profile.revision, Some(1));
    assert_eq!(set.records.len(), 1);
    let record = &set.records[0];
    assert_eq!(record.domain, "kept.com");
    assert_eq!(record.profile.id, alpha.0);
    assert_eq!(
        record.contact.as_ref().unwrap().address,
        "security@kept.com"
    );
    let assessment = record.assessment.as_ref().unwrap();
    assert_eq!(assessment.draft.as_ref().unwrap().status, "pending");
    assert_eq!(assessment.evidence[0].quote, "q");
}

#[sqlx::test(migrations = "../../migrations")]
async fn filters_and_suppression_apply_before_output(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    lead(&pool, profile, "high.com", "validated", 9.0).await;
    lead(&pool, profile, "low.com", "validated", 1.0).await;
    lead(&pool, profile, "fresh.com", "new", 9.0).await;
    lead(&pool, profile, "gone.com", "validated", 9.0).await;
    suppress(&pool, "domain", "gone.com").await;

    let set = LodestoneUseCase::new(pool.clone())
        .lead_records(
            profile,
            LeadQuery {
                status: Some(LeadStatus::Validated),
                min_score: Some(5.0),
                ..query()
            },
        )
        .await
        .unwrap();

    let domains: Vec<_> = set.records.iter().map(|r| r.domain.as_str()).collect();
    assert_eq!(domains, vec!["high.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_address_is_never_output_and_the_next_contact_is_used(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let id = lead(&pool, profile, "kept.com", "validated", 9.0).await;
    contact(&pool, id, "security@kept.com", 0.95).await;
    contact(&pool, id, "contact@kept.com", 0.5).await;
    suppress(&pool, "email", "security@kept.com").await;

    let lookup = LodestoneUseCase::new(pool.clone())
        .lead_record(id)
        .await
        .unwrap();

    let RecordLookup::Found(record) = lookup else {
        panic!("record expected");
    };
    assert_eq!(record.contact.unwrap().address, "contact@kept.com");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_or_unknown_lead_yields_no_record(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let id = lead(&pool, profile, "gone.com", "validated", 9.0).await;
    suppress(&pool, "domain", "gone.com").await;
    let usecase = LodestoneUseCase::new(pool.clone());

    assert_eq!(
        usecase.lead_record(id).await.unwrap(),
        RecordLookup::Suppressed
    );
    assert_eq!(
        usecase.lead_record(LeadId(Uuid::now_v7())).await.unwrap(),
        RecordLookup::Missing
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_profile_is_not_found(pool: PgPool) {
    let error = LodestoneUseCase::new(pool)
        .lead_records(ProfileId(Uuid::now_v7()), query())
        .await
        .unwrap_err();
    assert!(matches!(error, common::CoreError::NotFound));
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_a_draft_enqueues_a_webhook_only_when_enabled(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let first = lead(&pool, profile, "first.com", "qualified", 9.0).await;
    let first_assessment = assessment(&pool, first).await;
    let second = lead(&pool, profile, "second.com", "qualified", 9.0).await;
    let second_assessment = assessment(&pool, second).await;

    LodestoneUseCase::new(pool.clone())
        .approve_draft(lodestone_core::AssessmentId(first_assessment), actor)
        .await
        .unwrap();
    assert!(webhook_jobs(&pool).await.is_empty());

    LodestoneUseCase::new(pool.clone())
        .with_webhooks(Some(policy(vec![])))
        .approve_draft(lodestone_core::AssessmentId(second_assessment), actor)
        .await
        .unwrap();
    let jobs = webhook_jobs(&pool).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["lead_id"], second.0.to_string());
    assert_eq!(jobs[0]["event"], "lead.validated");
    assert!(jobs[0]["occurred_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_outside_the_filter_enqueues_nothing(pool: PgPool) {
    let alpha = create_profile(&pool, "alpha").await;
    let beta = create_profile(&pool, "beta").await;
    let actor = user(&pool).await;
    let id = lead(&pool, alpha, "a.com", "qualified", 9.0).await;
    let assessment_id = assessment(&pool, id).await;

    LodestoneUseCase::new(pool.clone())
        .with_webhooks(Some(policy(vec![beta])))
        .approve_draft(lodestone_core::AssessmentId(assessment_id), actor)
        .await
        .unwrap();

    assert!(webhook_jobs(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn moving_a_lead_to_validated_enqueues_a_webhook_and_other_moves_do_not(pool: PgPool) {
    let profile = create_profile(&pool, "alpha").await;
    let actor = user(&pool).await;
    let id = lead(&pool, profile, "a.com", "qualified", 9.0).await;
    let usecase = LodestoneUseCase::new(pool.clone()).with_webhooks(Some(policy(vec![profile])));

    usecase
        .change_lead_status(id, LeadStatus::Rejected, actor)
        .await
        .unwrap();
    assert!(webhook_jobs(&pool).await.is_empty());

    usecase
        .change_lead_status(id, LeadStatus::New, actor)
        .await
        .unwrap();
    usecase
        .change_lead_status(id, LeadStatus::Qualified, actor)
        .await
        .unwrap();
    usecase
        .change_lead_status(id, LeadStatus::Validated, actor)
        .await
        .unwrap();
    assert_eq!(webhook_jobs(&pool).await.len(), 1);

    let disabled = LodestoneUseCase::new(pool.clone());
    let other = lead(&pool, profile, "b.com", "qualified", 9.0).await;
    disabled
        .change_lead_status(other, LeadStatus::Validated, actor)
        .await
        .unwrap();
    assert_eq!(webhook_jobs(&pool).await.len(), 1);
}
