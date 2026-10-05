mod support;

use std::{num::NonZeroU16, time::Duration};

use chrono::{Duration as Delta, Utc};
use http::{Method, StatusCode};
use lodestone_core::{
    JobKind, JobQueue, NewJob, Run, RunId, RunOutcome, RunRepository, SourceName,
    infrastructure::{
        job::postgres::PgJobQueue, postgres::with_tx, run::postgres::PgRunRepository,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, send};
use utoipa::OpenApi;

struct Api {
    harness: Harness,
    bearer: String,
    queue: PgJobQueue,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool.clone()).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self {
            harness,
            bearer,
            queue: PgJobQueue::new(pool),
        }
    }

    async fn schedule(&self, kind: &str, key: &str, in_hours: i64) {
        self.queue
            .enqueue(
                &NewJob {
                    kind: JobKind::parse(kind).unwrap(),
                    payload: json!({}),
                    run_at: Utc::now() + Delta::hours(in_hours),
                    max_attempts: NonZeroU16::new(3).unwrap(),
                    dedup_key: Some(key.to_owned()),
                },
                Utc::now(),
            )
            .await
            .unwrap();
    }

    async fn call(&self, method: Method, uri: &str) -> (StatusCode, Value) {
        let (status, json, _) =
            send(self.harness.public(), method, uri, Some(&self.bearer), None).await;
        (status, json)
    }

    async fn list(&self) -> Vec<Value> {
        let (status, body) = self.call(Method::GET, "/api/v1/sources").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["data"].as_array().unwrap().clone()
    }
}

fn entry<'a>(sources: &'a [Value], name: &str) -> &'a Value {
    sources
        .iter()
        .find(|source| source["source"] == name)
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_moves_run_at_and_keeps_the_job_pending(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", 6).await;

    let (status, body) = api.call(Method::POST, "/api/v1/sources/boamp/run").await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["status"], "pending");
    let next = chrono::DateTime::parse_from_rfc3339(body["data"]["next_run_at"].as_str().unwrap())
        .unwrap();
    assert!(next <= Utc::now());
    let listed = api.list().await;
    assert_eq!(entry(&listed, "boamp")["status"], "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_refuses_a_running_job(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", -1).await;
    api.queue
        .claim(Utc::now(), Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();

    let (status, body) = api.call(Method::POST, "/api/v1/sources/boamp/run").await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["message"], "Conflict: source is already running");
    let listed = api.list().await;
    let boamp = entry(&listed, "boamp");
    assert_eq!(boamp["status"], "running");
    assert!(boamp["running_since"].is_string());
    assert!(boamp["lease_until"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_source_is_404_and_malformed_is_400(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, _) = api.call(Method::POST, "/api/v1/sources/nope/run").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = api.call(Method::POST, "/api/v1/sources/No-Pe/run").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_without_a_job_is_404_with_a_reason(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, body) = api
        .call(Method::POST, "/api/v1/sources/france_travail/run")
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], "source has no scheduled job");
}

#[sqlx::test(migrations = "../../migrations")]
async fn qualification_can_be_run_now(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("llm.qualify", "llm:qualify", 6).await;

    let (status, body) = api
        .call(Method::POST, "/api/v1/sources/llm_qualification/run")
        .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["kind"], "llm.qualify");
}

#[sqlx::test(migrations = "../../migrations")]
async fn requests_without_a_token_are_401(pool: PgPool) {
    let api = Api::new(pool).await;
    for (method, uri) in [
        (Method::GET, "/api/v1/sources"),
        (Method::POST, "/api/v1/sources/boamp/run"),
    ] {
        let (status, _, _) = send(api.harness.public(), method.clone(), uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_accounts_cannot_run_a_source(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", 6).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());

    let (status, _, _) = send(
        api.harness.public(),
        Method::POST,
        "/api/v1/sources/boamp/run",
        Some(&client),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    let listed = api.list().await;
    assert!(
        chrono::DateTime::parse_from_rfc3339(
            entry(&listed, "boamp")["next_run_at"].as_str().unwrap()
        )
        .unwrap()
            > Utc::now()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_shows_every_source_with_states_and_last_runs(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    api.schedule("source.run", "source:boamp", 6).await;
    api.schedule("llm.qualify", "llm:qualify", 1).await;
    let profile = support_profile(&pool).await;
    with_tx(&pool, async |shared| {
        let mut repo = PgRunRepository::new(&shared);
        let run = Run::start(
            RunId(common::generate_uuid_v7()),
            profile,
            SourceName::Boamp,
            None,
            Utc::now() - Delta::minutes(5),
        );
        repo.insert(&run).await?;
        repo.finish(
            run.id,
            &RunOutcome::Failed {
                volume: 12,
                signals: 3,
                error: "timeout".to_owned(),
            },
            Utc::now(),
        )
        .await?;
        Ok(())
    })
    .await
    .unwrap();

    let listed = api.list().await;

    assert_eq!(listed.len(), SourceName::ALL.len());
    let boamp = entry(&listed, "boamp");
    assert_eq!(boamp["status"], "pending");
    assert_eq!(boamp["kind"], "source.run");
    assert_eq!(boamp["last_runs"][0]["profile_name"], "alpha");
    assert_eq!(boamp["last_runs"][0]["status"], "failed");
    assert_eq!(boamp["last_runs"][0]["volume"], 12);
    assert_eq!(boamp["last_runs"][0]["signals"], 3);
    assert_eq!(boamp["last_runs"][0]["error"], "timeout");
    let qualification = entry(&listed, "llm_qualification");
    assert_eq!(qualification["status"], "pending");
    assert_eq!(qualification["kind"], "llm.qualify");
    let france_travail = entry(&listed, "france_travail");
    assert_eq!(france_travail["status"], "unavailable");
    assert_eq!(france_travail["unavailable_reason"], "missing_credentials");
    assert_eq!(
        entry(&listed, "decp")["unavailable_reason"],
        "not_scheduled"
    );
}

#[test]
fn the_endpoints_are_documented() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).unwrap();
    assert!(spec["paths"]["/api/v1/sources"]["get"].is_object());
    assert!(spec["paths"]["/api/v1/sources/{source}/run"]["post"].is_object());
}

async fn support_profile(pool: &PgPool) -> lodestone_core::ProfileId {
    let id = common::generate_uuid_v7();
    sqlx::query(
        "INSERT INTO profiles (id, name, created_at, updated_at) VALUES ($1, 'alpha', now(), now())",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    lodestone_core::ProfileId(id)
}
