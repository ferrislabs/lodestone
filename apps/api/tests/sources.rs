mod support;

use std::{collections::BTreeMap, num::NonZeroU16, time::Duration};

use chrono::{DateTime, Duration as Delta, Utc};
use common::Secret;
use http::{Method, StatusCode};
use lodestone_core::{
    JobKind, JobQueue, NewJob, ProviderStatus, Run, RunId, RunOutcome, RunRepository, SourceName,
    infrastructure::{
        job::postgres::PgJobQueue, postgres::with_tx, run::postgres::PgRunRepository,
    },
    provider_descriptor,
    source_providers::{SaveProviderConfiguration, SecretChange},
};
use serde_json::{Map, Value, json};
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

    async fn ready(&self, key: &str) {
        let usecase = &self.harness.state.usecase;
        let (settings, secrets) = if key == "france_travail" {
            (
                Map::from_iter([("client_id".to_owned(), json!("client-1"))]),
                BTreeMap::from([(
                    "client_secret".to_owned(),
                    SecretChange::Replace(Secret::new("secret-0123456789")),
                )]),
            )
        } else {
            (Map::new(), BTreeMap::new())
        };
        usecase
            .save_provider_configuration(
                provider_descriptor(key).unwrap(),
                SaveProviderConfiguration { settings, secrets },
                true,
                self.harness.state.cipher.as_ref(),
            )
            .await
            .unwrap();
        usecase
            .record_provider_status(key, ProviderStatus::Ready, None, Utc::now())
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
    api.ready("boamp").await;

    let (status, body) = api.call(Method::POST, "/api/v1/sources/boamp/run").await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["status"], "pending");
    let next = DateTime::parse_from_rfc3339(body["data"]["next_run_at"].as_str().unwrap()).unwrap();
    assert!(next <= Utc::now());
    let listed = api.list().await;
    assert_eq!(entry(&listed, "boamp")["status"], "pending");
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_refuses_a_running_job(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", -1).await;
    api.ready("boamp").await;
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
async fn run_now_is_refused_in_french_while_the_provider_is_not_ready(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", 6).await;

    let (status, body) = api.call(Method::POST, "/api/v1/sources/boamp/run").await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["message"],
        "Conflict: Le fournisseur « BOAMP » n'est pas prêt : configurez-le et testez-le dans le catalogue avant de lancer la source."
    );
    let listed = api.list().await;
    assert_eq!(entry(&listed, "boamp")["status"], "pending");
    let next =
        DateTime::parse_from_rfc3339(entry(&listed, "boamp")["next_run_at"].as_str().unwrap())
            .unwrap();
    assert!(next > Utc::now());
}

#[sqlx::test(migrations = "../../migrations")]
async fn run_now_is_refused_while_the_provider_is_failing(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("source.run", "source:boamp", 6).await;
    api.ready("boamp").await;
    api.harness
        .state
        .usecase
        .record_provider_status("boamp", ProviderStatus::Failing, None, Utc::now())
        .await
        .unwrap();

    let (status, _) = api.call(Method::POST, "/api/v1/sources/boamp/run").await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_carries_the_provider_of_every_source(pool: PgPool) {
    let api = Api::new(pool).await;
    api.ready("decp").await;

    let listed = api.list().await;

    assert_eq!(
        entry(&listed, "boamp")["provider"],
        json!({"key": "boamp", "name": "BOAMP", "status": "not_configured"})
    );
    assert_eq!(entry(&listed, "decp")["provider"]["status"], "ready");
    assert_eq!(
        entry(&listed, "security_txt")["provider"]["key"],
        "contacts"
    );
    assert_eq!(entry(&listed, "role_guess")["provider"]["key"], "contacts");
    assert_eq!(
        entry(&listed, "company_research")["provider"]["key"],
        "company_research"
    );
    assert!(entry(&listed, "qualification")["provider"].is_null());
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
    api.ready("france_travail").await;

    let (status, body) = api
        .call(Method::POST, "/api/v1/sources/france_travail/run")
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["message"], "source has no scheduled job");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_former_qualification_source_is_gone(pool: PgPool) {
    let api = Api::new(pool).await;
    api.schedule("llm.qualify", "llm:qualify", 6).await;

    let (status, body) = api
        .call(Method::POST, "/api/v1/sources/llm_qualification/run")
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let listed = api.list().await;
    assert!(
        listed
            .iter()
            .all(|source| source["source"] != "llm_qualification")
    );
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
        DateTime::parse_from_rfc3339(entry(&listed, "boamp")["next_run_at"].as_str().unwrap())
            .unwrap()
            > Utc::now()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_shows_every_source_with_states_and_last_runs(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    api.schedule("source.run", "source:boamp", 6).await;
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

    assert_eq!(listed.len(), SourceName::ALL.len() + 2);
    let company = entry(&listed, "company_research");
    assert_eq!(company["mode"], "continuous");
    assert_eq!(company["kind"], "listener.company_research");
    let boamp = entry(&listed, "boamp");
    assert_eq!(boamp["status"], "pending");
    assert_eq!(boamp["kind"], "source.run");
    assert_eq!(boamp["last_runs"][0]["profile_name"], "alpha");
    assert_eq!(boamp["last_runs"][0]["status"], "failed");
    assert_eq!(boamp["last_runs"][0]["volume"], 12);
    assert_eq!(boamp["last_runs"][0]["signals"], 3);
    assert_eq!(boamp["last_runs"][0]["error"], "timeout");
    let france_travail = entry(&listed, "france_travail");
    assert_eq!(france_travail["status"], "unavailable");
    assert_eq!(france_travail["unavailable_reason"], "not_scheduled");
    assert_eq!(
        entry(&listed, "decp")["unavailable_reason"],
        "not_scheduled"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn continuous_sources_report_their_listener_and_refuse_run_now(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    api.schedule("source.run", "source:boamp", 6).await;
    sqlx::query(
        "INSERT INTO listeners (name, holder, state, last_activity_at, items_total, signals_total, backlog, updated_at)
         VALUES ('dns_cname', 'node-a-1', 'listening', now(), 40, 5, 12, now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listeners (name, holder, state, last_error, updated_at)
         VALUES ('http_probe', 'node-a-1', 'error', 'dns down', now())",
    )
    .execute(&pool)
    .await
    .unwrap();

    let listed = api.list().await;

    let dns = entry(&listed, "dns_cname");
    assert_eq!(dns["mode"], "continuous");
    assert_eq!(dns["status"], "listening");
    assert_eq!(dns["listener"]["state"], "listening");
    assert_eq!(dns["listener"]["holder"], "node-a-1");
    assert_eq!(dns["listener"]["items_total"], 40);
    assert_eq!(dns["listener"]["signals_total"], 5);
    assert_eq!(dns["listener"]["backlog"], 12);
    assert!(dns["listener"]["last_activity_at"].is_string());
    assert!(dns["next_run_at"].is_null());
    let probe = entry(&listed, "http_probe");
    assert_eq!(probe["listener"]["state"], "error");
    assert_eq!(probe["listener"]["last_error"], "dns down");
    let silent = entry(&listed, "role_guess");
    assert_eq!(silent["mode"], "continuous");
    assert_eq!(silent["listener"]["state"], "stopped");
    for name in [
        "certificate_transparency",
        "dns_cname",
        "http_probe",
        "security_txt",
        "public_page",
        "role_guess",
    ] {
        assert_eq!(entry(&listed, name)["mode"], "continuous", "{name}");
    }
    let boamp = entry(&listed, "boamp");
    assert_eq!(boamp["mode"], "scheduled");
    assert!(boamp["listener"].is_null());
    assert_eq!(entry(&listed, "qualification")["mode"], "continuous");

    for name in ["dns_cname", "certificate_transparency", "role_guess"] {
        let (status, body) = api
            .call(Method::POST, &format!("/api/v1/sources/{name}/run"))
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["message"], "Conflict: source listens continuously");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_qualification_listener_is_listed_with_its_backlog(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    sqlx::query(
        "INSERT INTO listeners (name, holder, state, last_activity_at, items_total, signals_total, backlog, updated_at)
         VALUES ('qualification', 'node-a-1', 'listening', now(), 30, 28, 7, now())",
    )
    .execute(&pool)
    .await
    .unwrap();

    let listed = api.list().await;

    let qualification = entry(&listed, "qualification");
    assert_eq!(qualification["mode"], "continuous");
    assert_eq!(qualification["kind"], "listener.qualification");
    assert_eq!(qualification["label_key"], "source.qualification");
    assert_eq!(qualification["status"], "listening");
    assert_eq!(qualification["listener"]["backlog"], 7);
    assert_eq!(qualification["listener"]["items_total"], 30);
    assert!(qualification["next_run_at"].is_null());
    let (status, _) = api
        .call(Method::POST, "/api/v1/sources/qualification/run")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_silent_listener_is_reported_stale(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    sqlx::query(
        "INSERT INTO listeners (name, holder, state, updated_at) VALUES
         ('dns_cname', 'node-a-1', 'listening', now() - interval '10 minutes'),
         ('http_probe', 'node-a-1', 'listening', now() - interval '10 seconds'),
         ('role_guess', NULL, 'standby', now() - interval '10 minutes'),
         ('public_page', 'node-a-1', 'stopped', now() - interval '10 minutes')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let listed = api.list().await;

    assert_eq!(entry(&listed, "dns_cname")["listener"]["state"], "stale");
    assert_eq!(entry(&listed, "dns_cname")["status"], "stale");
    assert!(entry(&listed, "dns_cname")["listener"]["updated_at"].is_string());
    assert_eq!(
        entry(&listed, "http_probe")["listener"]["state"],
        "listening"
    );
    assert_eq!(entry(&listed, "role_guess")["listener"]["state"], "standby");
    assert_eq!(
        entry(&listed, "public_page")["listener"]["state"],
        "stopped"
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
