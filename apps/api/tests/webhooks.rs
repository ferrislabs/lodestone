use std::{num::NonZeroU16, time::Duration};

use api::webhook::{
    EVENT_ID_HEADER, SIGNATURE_HEADER, SIGNATURE_TOLERANCE_SECONDS, WebhookSink, run_webhook_job,
    verify,
};
use chrono::Utc;
use common::Secret;
use httpmock::{Method::POST, MockServer, prelude::HttpMockRequest};
use lodestone_core::{
    Job, JobId, JobKind, JobStatus, LeadEventKind, LeadId, LodestoneUseCase,
    jobs::{JobError, JobOutcome},
    webhook_job,
};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const SECRET: &str = "lodestone-test-secret-0123456789abcdef";

fn header<'a>(request: &'a HttpMockRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .as_ref()?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn is_signed_and_well_formed(request: &HttpMockRequest) -> bool {
    let (Some(signature), Some(event_id), Some(content_type), Some(body)) = (
        header(request, SIGNATURE_HEADER),
        header(request, EVENT_ID_HEADER),
        header(request, "content-type"),
        request.body.as_deref(),
    ) else {
        return false;
    };
    let Ok(envelope) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    content_type == "application/json"
        && envelope["id"] == event_id
        && verify(
            SECRET.as_bytes(),
            signature,
            body,
            Utc::now().timestamp(),
            SIGNATURE_TOLERANCE_SECONDS,
        )
        .is_ok()
}

async fn lead(pool: &PgPool, domain: &str) -> (Uuid, Uuid) {
    let profile = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO profiles (id, name, active, created_at, updated_at) VALUES ($1, 'alpha', true, now(), now())",
    )
    .bind(profile)
    .execute(pool)
    .await
    .unwrap();
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
         VALUES ($1, $2, $3, 'validated', 7, now(), now(), now())",
    )
    .bind(id)
    .bind(profile)
    .bind(domain)
    .execute(pool)
    .await
    .unwrap();
    (profile, id)
}

fn job_for(lead: Uuid) -> Job {
    let new = webhook_job(
        LeadId(lead),
        LeadEventKind::Validated,
        Utc::now(),
        NonZeroU16::new(5).unwrap(),
    )
    .unwrap();
    let now = Utc::now();
    Job {
        id: JobId(Uuid::now_v7()),
        kind: JobKind::parse("webhook.deliver").unwrap(),
        payload: new.payload,
        status: JobStatus::Running,
        attempts: 1,
        max_attempts: NonZeroU16::new(5).unwrap(),
        run_at: now,
        last_error: None,
        created_at: now,
        updated_at: now,
    }
}

fn sink(server: &MockServer) -> WebhookSink {
    WebhookSink::new(
        server.url("/hook").parse().unwrap(),
        Secret::new(SECRET),
        Duration::from_secs(5),
    )
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_validated_lead_is_posted_signed_with_the_documented_envelope(pool: PgPool) {
    let (profile, lead) = lead(&pool, "alpha.com").await;
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/hook")
                .header_exists("lodestone-signature")
                .header_exists("lodestone-event-id")
                .header("content-type", "application/json")
                .matches(is_signed_and_well_formed)
                .body_contains("\"type\":\"lead.validated\"")
                .body_contains(format!("\"profile_id\":\"{profile}\""))
                .body_contains(format!("\"lead_id\":\"{lead}\""))
                .body_contains("\"domain\":\"alpha.com\"");
            then.status(204);
        })
        .await;

    let outcome = run_webhook_job(
        &LodestoneUseCase::new(pool),
        Some(&sink(&server)),
        &job_for(lead),
    )
    .await;

    assert_eq!(outcome, Ok(JobOutcome::Done));
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_server_error_is_retried(pool: PgPool) {
    let (_, lead) = lead(&pool, "alpha.com").await;
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST).path("/hook");
            then.status(500);
        })
        .await;

    let outcome = run_webhook_job(
        &LodestoneUseCase::new(pool),
        Some(&sink(&server)),
        &job_for(lead),
    )
    .await;

    assert!(matches!(outcome, Err(JobError::Retryable(reason)) if reason.contains("500")));
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unreachable_endpoint_is_retried_without_leaking_its_url(pool: PgPool) {
    let (_, lead) = lead(&pool, "alpha.com").await;
    let dead = WebhookSink::new(
        "http://127.0.0.1:1/secret-token-path".parse().unwrap(),
        Secret::new(SECRET),
        Duration::from_secs(2),
    )
    .unwrap();

    let outcome = run_webhook_job(&LodestoneUseCase::new(pool), Some(&dead), &job_for(lead)).await;

    match outcome {
        Err(JobError::Retryable(reason)) => assert!(!reason.contains("secret-token-path")),
        other => panic!("retry expected, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_gone_endpoint_completes_the_job(pool: PgPool) {
    let (_, lead) = lead(&pool, "alpha.com").await;
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST).path("/hook");
            then.status(410);
        })
        .await;

    let outcome = run_webhook_job(
        &LodestoneUseCase::new(pool),
        Some(&sink(&server)),
        &job_for(lead),
    )
    .await;

    assert_eq!(outcome, Ok(JobOutcome::Done));
    mock.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_redirect_is_not_followed_and_is_retried(pool: PgPool) {
    let (_, lead) = lead(&pool, "alpha.com").await;
    let server = MockServer::start_async().await;
    let target = server
        .mock_async(|when, then| {
            when.method(POST).path("/elsewhere");
            then.status(200);
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/hook");
            then.status(307).header("location", "/elsewhere");
        })
        .await;

    let outcome = run_webhook_job(
        &LodestoneUseCase::new(pool),
        Some(&sink(&server)),
        &job_for(lead),
    )
    .await;

    assert!(matches!(outcome, Err(JobError::Retryable(_))));
    target.assert_hits_async(0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_lead_completes_without_sending(pool: PgPool) {
    let (_, lead) = lead(&pool, "gone.com").await;
    sqlx::query(
        "INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'domain', 'gone.com', now())",
    )
    .bind(Uuid::now_v7())
    .execute(&pool)
    .await
    .unwrap();
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST);
            then.status(200);
        })
        .await;

    let outcome = run_webhook_job(
        &LodestoneUseCase::new(pool),
        Some(&sink(&server)),
        &job_for(lead),
    )
    .await;

    assert_eq!(outcome, Ok(JobOutcome::Done));
    mock.assert_hits_async(0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_vanished_lead_or_disabled_webhooks_complete_without_sending(pool: PgPool) {
    let (_, lead) = lead(&pool, "alpha.com").await;
    let server = MockServer::start_async().await;
    let mock = server
        .mock_async(|when, then| {
            when.method(POST);
            then.status(200);
        })
        .await;
    let usecase = LodestoneUseCase::new(pool);

    let missing = run_webhook_job(&usecase, Some(&sink(&server)), &job_for(Uuid::now_v7())).await;
    let disabled = run_webhook_job(&usecase, None::<&WebhookSink>, &job_for(lead)).await;

    assert_eq!(missing, Ok(JobOutcome::Done));
    assert_eq!(disabled, Ok(JobOutcome::Done));
    mock.assert_hits_async(0).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_payload_fails_permanently(pool: PgPool) {
    let mut job = job_for(Uuid::now_v7());
    job.payload = serde_json::json!({"nope": true});

    let outcome = run_webhook_job(&LodestoneUseCase::new(pool), None::<&WebhookSink>, &job).await;

    assert!(matches!(outcome, Err(JobError::Permanent(_))));
}
