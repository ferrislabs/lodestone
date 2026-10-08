mod support;

use api::{crawl::refresh_robots_policy, openapi::ApiDoc};
use chrono::{Duration, Utc};
use common::RobotsPolicy;
use fetch::RobotsGate;
use http::{Method, StatusCode};
use lodestone_core::{CreateUserCommand, LodestoneUseCase};
use serde_json::{Value, json, to_value};
use sqlx::{PgPool, query, query_scalar};
use support::{Harness, send};
use utoipa::OpenApi;

const URI: &str = "/api/v1/settings/crawl";

async fn call(harness: &Harness, method: Method, body: Option<Value>) -> (StatusCode, Value) {
    let bearer = format!("Bearer {}", harness.valid_user_token());
    let (status, json, _) = send(harness.public(), method, URI, Some(&bearer), body).await;
    (status, json)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_crawl_settings_require_a_user(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let client = format!("Bearer {}", harness.valid_client_token());
    for method in [Method::GET, Method::PUT] {
        let body = (method == Method::PUT).then(|| json!({"robots_policy": "ignore"}));
        let (anonymous, _, _) =
            send(harness.public(), method.clone(), URI, None, body.clone()).await;
        assert_eq!(anonymous, StatusCode::UNAUTHORIZED, "{method}");
        let (service, _, _) =
            send(harness.public(), method.clone(), URI, Some(&client), body).await;
        assert_eq!(service, StatusCode::FORBIDDEN, "{method}");
    }
    assert_eq!(harness.state.robots.policy(), RobotsPolicy::Respect);
}

#[sqlx::test(migrations = "../../migrations")]
async fn robots_are_respected_until_an_operator_saves_ignore(pool: PgPool) {
    let harness = Harness::new(pool).await;

    let (status, body) = call(&harness, Method::GET, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["robots_policy"], "respect");
    assert_eq!(body["data"]["origin"], "environment");
    assert_eq!(body["data"]["environment_default"], "respect");
    assert_eq!(body["data"]["updated_by_name"], Value::Null);
    assert_eq!(body["data"]["robots_ignored"], 0);

    let (status, body) = call(
        &harness,
        Method::PUT,
        Some(json!({"robots_policy": "ignore"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["settings"]["robots_policy"], "ignore");
    assert_eq!(body["data"]["settings"]["origin"], "organisation");
    assert_eq!(body["data"]["settings"]["updated_by_name"], "Alice");
    assert!(body["data"]["settings"]["updated_by"].is_string());
    assert!(body["data"]["settings"]["updated_at"].is_string());
    assert_eq!(harness.state.robots.policy(), RobotsPolicy::Ignore);

    let (_, body) = call(&harness, Method::GET, None).await;
    assert_eq!(body["data"]["robots_policy"], "ignore");
    assert_eq!(body["data"]["updated_by_name"], "Alice");

    call(
        &harness,
        Method::PUT,
        Some(json!({"robots_policy": "respect"})),
    )
    .await;
    assert_eq!(harness.state.robots.policy(), RobotsPolicy::Respect);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_policy_is_rejected(pool: PgPool) {
    let harness = Harness::new(pool).await;

    let (status, body) = call(
        &harness,
        Method::PUT,
        Some(json!({"robots_policy": "sometimes"})),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["details"][0]["path"], "robots_policy");
    assert_eq!(harness.state.robots.policy(), RobotsPolicy::Respect);
}

#[sqlx::test(migrations = "../../migrations")]
async fn saving_ignore_schedules_the_dossiers_refused_by_robots(pool: PgPool) {
    let harness = Harness::new(pool.clone()).await;
    let later = Utc::now() + Duration::days(30);
    query(
        "INSERT INTO company_dossiers (domain, state, pages, attempts, next_attempt_at, error)
         VALUES ('refused.fr', 'unreachable', '[]'::jsonb, 4, $1, 'robots.txt disallows the home page')",
    )
    .bind(later)
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) = call(
        &harness,
        Method::PUT,
        Some(json!({"robots_policy": "ignore"})),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["dossiers_retried"], 1);
    let due: bool = query_scalar(
        "SELECT attempts = 0 AND next_attempt_at <= now() FROM company_dossiers WHERE domain = 'refused.fr'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(due);
}

#[test]
fn the_crawl_settings_are_documented() {
    let doc = to_value(ApiDoc::openapi()).unwrap();
    let path = &doc["paths"][URI];
    assert!(path["get"].is_object());
    assert!(path["put"].is_object());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refresh_picks_up_a_policy_saved_by_another_instance(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool);
    let gate = RobotsGate::default();

    assert_eq!(
        refresh_robots_policy(&usecase, &gate, RobotsPolicy::Ignore).await,
        RobotsPolicy::Ignore
    );
    assert_eq!(gate.policy(), RobotsPolicy::Ignore);

    let user = usecase
        .create_user(CreateUserCommand {
            name: "Bob".to_owned(),
            username: "bob".to_owned(),
            email: None,
            sub: "subject-7".to_owned(),
        })
        .await
        .unwrap()
        .id;
    usecase
        .update_crawl_settings(
            RobotsPolicy::Respect,
            user,
            Utc::now(),
            RobotsPolicy::Ignore,
        )
        .await
        .unwrap();

    refresh_robots_policy(&usecase, &gate, RobotsPolicy::Ignore).await;
    assert_eq!(gate.policy(), RobotsPolicy::Respect);
}
