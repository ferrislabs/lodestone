mod support;

use axum::Router;
use http::{Method, StatusCode, header};
use lodestone_core::{CancelReason, WorkKind, WorkTarget, definition::AgentId};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, send, send_raw};
use utoipa::OpenApi;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const ECOMMERCE: &str =
    include_str!("../../../docs/examples/profiles/ecommerce-replatforming.json");

fn iam() -> Value {
    serde_json::from_str(IAM).expect("iam example")
}

fn canonical(value: &Value) -> Value {
    lodestone_core::ValidatedProfile::parse(value.clone())
        .expect("valid definition")
        .to_json()
}

fn ecommerce() -> Value {
    serde_json::from_str(ECOMMERCE).expect("ecommerce example")
}

struct Api {
    harness: Harness,
    bearer: String,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self { harness, bearer }
    }

    fn app(&self) -> Router {
        self.harness.public()
    }

    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, json, _) = send(self.app(), method, uri, Some(&self.bearer), body).await;
        (status, json)
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.call(Method::GET, uri, None).await
    }

    async fn create(&self, name: &str, definition: Value) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/api/v1/profiles",
                Some(json!({ "name": name, "definition": definition })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["id"].as_str().expect("id").to_owned()
    }

    async fn update(&self, id: &str, definition: Value) -> (StatusCode, Value) {
        self.call(
            Method::PUT,
            &format!("/api/v1/profiles/{id}"),
            Some(json!({ "definition": definition })),
        )
        .await
    }
}

fn with_offer(mut definition: Value, offer: &str) -> Value {
    definition["identity"]["offer"] = json!(offer);
    definition
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_returns_201_with_location_and_an_inactive_first_revision(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, headers, raw) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/profiles",
        Some(&api.bearer),
        Some(json!({ "name": "iam", "definition": iam() }).to_string()),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{raw}");
    let body: Value = serde_json::from_str(&raw).expect("json");
    let id = body["data"]["id"].as_str().expect("id");
    assert_eq!(
        headers[header::LOCATION],
        format!("/api/v1/profiles/{id}").as_str()
    );
    assert_eq!(body["data"]["name"], "iam");
    assert_eq!(body["data"]["active"], false);
    assert_eq!(body["data"]["revision"], 1);
    assert_eq!(canonical(&body["data"]["definition"]), canonical(&iam()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_without_a_name_uses_the_offer(pool: PgPool) {
    let api = Api::new(pool).await;
    let mut definition = iam();
    definition["identity"]["offer"] = json!("Managed IAM");

    let (status, body) = api
        .call(
            Method::POST,
            "/api/v1/profiles",
            Some(json!({ "definition": definition })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["name"], "Managed IAM");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_definitions_are_rejected_with_the_faulty_field_path(pool: PgPool) {
    let api = Api::new(pool).await;

    let mut bad_influence = iam();
    bad_influence["evidence"]["influence"] = json!("high");
    let mut bad_version = iam();
    bad_version["schema_version"] = json!(99);
    let mut unknown_field = iam();
    unknown_field["identity"]["colour"] = json!("red");
    let mut empty_offer = iam();
    empty_offer["identity"]["offer"] = json!("");

    let mut removed_weights = iam();
    removed_weights["scoring"] = json!({"factors": []});
    let mut removed_reference = iam();
    removed_reference["evidence"]["reference_score"] = json!(50);
    let mut removed_llm = iam();
    removed_llm["llm"] = json!({"language": "fr"});

    let cases = [
        (bad_influence, "evidence.influence"),
        (removed_weights, "scoring"),
        (removed_reference, "evidence.reference_score"),
        (removed_llm, "llm"),
        (bad_version, "schema_version"),
        (unknown_field, "identity.colour"),
    ];
    for (definition, path) in cases {
        let (status, body) = api
            .call(
                Method::POST,
                "/api/v1/profiles",
                Some(json!({ "name": "bad", "definition": definition })),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["code"], "E_UNPROCESSABLE_ENTITY");
        assert_eq!(body["details"][0]["path"], path, "{body}");
        assert!(body["details"][0]["message"].is_string());
    }

    let (status, body) = api
        .call(
            Method::POST,
            "/api/v1/profiles",
            Some(json!({ "name": "bad", "definition": empty_offer })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"][0]["path"], "identity.offer");

    let (_, listing) = api.get("/api/v1/profiles").await;
    assert_eq!(listing["data"].as_array().expect("array").len(), 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn malformed_json_and_envelopes_are_rejected_with_a_path(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, _, raw) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/profiles",
        Some(&api.bearer),
        Some("{not json".to_owned()),
    )
    .await;
    let body: Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"][0]["path"], "$");

    let (status, body) = api
        .call(
            Method::POST,
            "/api/v1/profiles",
            Some(json!({ "name": "x" })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"][0]["path"], "definition");

    let (status, body) = api
        .call(
            Method::POST,
            "/api/v1/profiles",
            Some(json!({ "name": 3, "definition": iam() })),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"][0]["path"], "name");
}

#[sqlx::test(migrations = "../../migrations")]
async fn update_appends_a_revision_and_leaves_the_past_one_intact(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;
    let changed = with_offer(iam(), "Managed IAM v2");

    let (status, body) = api.update(&id, changed.clone()).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["number"], 2);
    let (_, first) = api.get(&format!("/api/v1/profiles/{id}/revisions/1")).await;
    assert_eq!(canonical(&first["data"]["definition"]), canonical(&iam()));
    let (_, second) = api.get(&format!("/api/v1/profiles/{id}/revisions/2")).await;
    assert_eq!(
        canonical(&second["data"]["definition"]),
        canonical(&changed)
    );
    let (_, current) = api.get(&format!("/api/v1/profiles/{id}")).await;
    assert_eq!(current["data"]["revision"], 2);
    assert_eq!(
        canonical(&current["data"]["definition"]),
        canonical(&changed)
    );
    let (_, listing) = api.get(&format!("/api/v1/profiles/{id}/revisions")).await;
    let numbers: Vec<_> = listing["data"]
        .as_array()
        .expect("array")
        .iter()
        .map(|revision| revision["number"].clone())
        .collect();
    assert_eq!(numbers, [json!(1), json!(2)]);
    assert!(listing["data"][0]["created_at"].is_string());
    assert!(listing["data"][0]["created_by"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_update_creates_no_revision(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;
    let mut bad = iam();
    bad["schema_version"] = json!(18);

    let (status, body) = api.update(&id, bad).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["details"][0]["path"], "schema_version");
    let (_, listing) = api.get(&format!("/api/v1/profiles/{id}/revisions")).await;
    assert_eq!(listing["data"].as_array().expect("array").len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn identical_update_keeps_the_current_revision(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;

    let (status, body) = api.update(&id, iam()).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["number"], 1);
    let (_, listing) = api.get(&format!("/api/v1/profiles/{id}/revisions")).await;
    assert_eq!(listing["data"].as_array().expect("array").len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivation_stops_scheduling_but_keeps_the_leads(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let id = api.create("iam", iam()).await;
    sqlx::query(
        "INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
         VALUES ($1, $2::uuid, 'alpha.com', 'new', 10, now(), now(), now())",
    )
    .bind(common::generate_uuid_v7())
    .bind(&id)
    .execute(&pool)
    .await
    .expect("lead");
    let scheduled = || async {
        api.harness
            .state
            .usecase
            .list_active_profiles()
            .await
            .expect("active")
            .iter()
            .any(|active| active.profile.id.to_string() == id)
    };
    assert!(!scheduled().await);

    let (status, body) = api
        .call(
            Method::POST,
            &format!("/api/v1/profiles/{id}/activate"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["active"], true);
    assert!(scheduled().await);

    let (status, body) = api
        .call(
            Method::POST,
            &format!("/api/v1/profiles/{id}/deactivate"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["active"], false);
    assert!(!scheduled().await);
    let (status, leads) = api.get(&format!("/api/v1/profiles/{id}/leads")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(leads["data"][0]["domain"], "alpha.com");
    let (_, listing) = api.get("/api/v1/profiles").await;
    assert_eq!(listing["data"][0]["lead_count"], 1);

    let (_, body) = api
        .call(
            Method::POST,
            &format!("/api/v1/profiles/{id}/activate"),
            None,
        )
        .await;
    assert_eq!(body["data"]["active"], true);
    assert!(scheduled().await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn export_then_import_round_trips_the_definition(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam export", iam()).await;

    let (status, headers, raw) = send_raw(
        api.app(),
        Method::GET,
        &format!("/api/v1/profiles/{id}/export"),
        Some(&api.bearer),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"iam_export.json\""
    );
    let exported: Value = serde_json::from_str(&raw).expect("json");
    assert_eq!(canonical(&exported), canonical(&iam()));

    let (status, headers, body) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/profiles/import?name=copy",
        Some(&api.bearer),
        Some(raw),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let created: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(created["data"]["name"], "copy");
    assert_eq!(created["data"]["definition"], exported);
    assert_eq!(
        headers[header::LOCATION],
        format!(
            "/api/v1/profiles/{}",
            created["data"]["id"].as_str().expect("id")
        )
        .as_str()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn both_example_profiles_import(pool: PgPool) {
    let api = Api::new(pool).await;

    for example in [IAM, ECOMMERCE] {
        let (status, _, body) = send_raw(
            api.app(),
            Method::POST,
            "/api/v1/profiles/import",
            Some(&api.bearer),
            Some(example.to_owned()),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    let (_, listing) = api.get("/api/v1/profiles").await;
    assert_eq!(listing["data"].as_array().expect("array").len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn import_reports_the_faulty_path_and_rejects_duplicate_names(pool: PgPool) {
    let api = Api::new(pool).await;
    let mut bad = iam();
    bad["evidence"]["influence"] = json!(null);
    let (status, _, body) = send_raw(
        api.app(),
        Method::POST,
        "/api/v1/profiles/import",
        Some(&api.bearer),
        Some(bad.to_string()),
    )
    .await;
    let parsed: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(parsed["details"][0]["path"], "evidence.influence");

    for expected in [StatusCode::CREATED, StatusCode::CONFLICT] {
        let (status, _, _) = send_raw(
            api.app(),
            Method::POST,
            "/api/v1/profiles/import",
            Some(&api.bearer),
            Some(IAM.to_owned()),
        )
        .await;
        assert_eq!(status, expected);
    }
    let (_, listing) = api.get("/api/v1/profiles").await;
    assert_eq!(listing["data"].as_array().expect("array").len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_names_on_create_conflict(pool: PgPool) {
    let api = Api::new(pool).await;
    api.create("iam", iam()).await;

    let (status, body) = api
        .call(
            Method::POST,
            "/api/v1/profiles",
            Some(json!({ "name": "iam", "definition": ecommerce() })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "E_CONFLICT");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_profiles_are_not_found(pool: PgPool) {
    let api = Api::new(pool).await;
    let missing = "0198a000-0000-7000-8000-000000000000";

    let reads = [
        format!("/api/v1/profiles/{missing}"),
        format!("/api/v1/profiles/{missing}/revisions"),
        format!("/api/v1/profiles/{missing}/revisions/1"),
        format!("/api/v1/profiles/{missing}/export"),
    ];
    for uri in reads {
        let (status, _) = api.get(&uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    for action in ["activate", "deactivate"] {
        let (status, _) = api
            .call(
                Method::POST,
                &format!("/api/v1/profiles/{missing}/{action}"),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{action}");
    }
    let (status, _) = api.update(missing, iam()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let id = api.create("iam", iam()).await;
    let (status, _) = api.get(&format!("/api/v1/profiles/{id}/revisions/9")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = api.get("/api/v1/profiles/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_accounts_can_read_but_not_mutate(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let body = json!({ "definition": iam() });

    let mutations = [
        (
            Method::POST,
            "/api/v1/profiles".to_owned(),
            Some(body.clone()),
        ),
        (
            Method::POST,
            "/api/v1/profiles/import".to_owned(),
            Some(iam()),
        ),
        (Method::PUT, format!("/api/v1/profiles/{id}"), Some(body)),
        (
            Method::POST,
            format!("/api/v1/profiles/{id}/activate"),
            None,
        ),
        (
            Method::POST,
            format!("/api/v1/profiles/{id}/deactivate"),
            None,
        ),
        (Method::DELETE, format!("/api/v1/profiles/{id}"), None),
    ];
    for (method, uri, payload) in mutations {
        let (status, _, _) = send(api.app(), method, &uri, Some(&client), payload).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    for uri in [
        format!("/api/v1/profiles/{id}"),
        format!("/api/v1/profiles/{id}/export"),
    ] {
        let (status, _, _) = send(api.app(), Method::GET, &uri, Some(&client), None).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_profile_route_requires_authentication(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = "0198a000-0000-7000-8000-000000000000";
    let routes = [
        (Method::GET, "/api/v1/profiles".to_owned()),
        (Method::POST, "/api/v1/profiles".to_owned()),
        (Method::POST, "/api/v1/profiles/import".to_owned()),
        (Method::GET, format!("/api/v1/profiles/{id}")),
        (Method::PUT, format!("/api/v1/profiles/{id}")),
        (Method::GET, format!("/api/v1/profiles/{id}/revisions")),
        (Method::GET, format!("/api/v1/profiles/{id}/revisions/1")),
        (Method::POST, format!("/api/v1/profiles/{id}/activate")),
        (Method::POST, format!("/api/v1/profiles/{id}/deactivate")),
        (Method::DELETE, format!("/api/v1/profiles/{id}")),
        (Method::GET, format!("/api/v1/profiles/{id}/export")),
    ];

    for (method, uri) in routes {
        let (status, _, _) = send(api.app(), method.clone(), &uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[test]
fn openapi_documents_every_profile_path_with_bearer_auth() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).expect("spec");
    let expected = [
        ("/api/v1/profiles", "get"),
        ("/api/v1/profiles", "post"),
        ("/api/v1/profiles/import", "post"),
        ("/api/v1/profiles/{profile_id}", "get"),
        ("/api/v1/profiles/{profile_id}", "put"),
        ("/api/v1/profiles/{profile_id}/revisions", "get"),
        ("/api/v1/profiles/{profile_id}/revisions/{number}", "get"),
        ("/api/v1/profiles/{profile_id}/activate", "post"),
        ("/api/v1/profiles/{profile_id}/deactivate", "post"),
        ("/api/v1/profiles/{profile_id}", "delete"),
        ("/api/v1/profiles/{profile_id}/export", "get"),
    ];

    for (path, method) in expected {
        let operation = &spec["paths"][path][method];
        assert!(operation.is_object(), "{method} {path} is undocumented");
        assert!(
            operation["security"][0].get("bearer_auth").is_some(),
            "{method} {path} lacks bearer_auth"
        );
    }
    assert!(spec["components"]["schemas"]["ErrorDetail"]["properties"]["path"].is_object());
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_a_profile_removes_it_and_stops_its_agents(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;
    let (status, _) = api
        .call(
            Method::POST,
            &format!("/api/v1/profiles/{id}/activate"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let usecase = &api.harness.state.usecase;
    let current = usecase.refreshed_active_profiles().await.unwrap().remove(0);
    let work = usecase.agent_work(&current, WorkTarget::aggregator());
    assert!(work.is_live());

    let (status, _) = api
        .call(Method::DELETE, &format!("/api/v1/profiles/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    assert_eq!(work.stop_reason(), Some(CancelReason::ProfileDeleted));
    let (status, _) = api.get(&format!("/api/v1/profiles/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = api
        .call(Method::DELETE, &format!("/api/v1/profiles/{id}"), None)
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivating_a_profile_stops_the_agents_of_the_running_instance(pool: PgPool) {
    let api = Api::new(pool).await;
    let id = api.create("iam", iam()).await;
    api.call(
        Method::POST,
        &format!("/api/v1/profiles/{id}/activate"),
        None,
    )
    .await;
    let usecase = &api.harness.state.usecase;
    let current = usecase.refreshed_active_profiles().await.unwrap().remove(0);
    let marche = usecase.agent_work(
        &current,
        WorkTarget::new(WorkKind::Qualification, &AgentId::new("marche")),
    );
    let pertinence = usecase.agent_work(
        &current,
        WorkTarget::new(WorkKind::Qualification, &AgentId::new("pertinence")),
    );
    let mut revised = iam();
    revised["qualification_agents"]
        .as_array_mut()
        .unwrap()
        .retain(|agent| agent["id"] != "marche");

    let (status, _) = api.update(&id, revised).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(marche.stop_reason(), Some(CancelReason::AgentRemoved));
    assert!(pertinence.is_live());

    api.call(
        Method::POST,
        &format!("/api/v1/profiles/{id}/deactivate"),
        None,
    )
    .await;
    assert_eq!(
        pertinence.stop_reason(),
        Some(CancelReason::ProfileDeactivated)
    );
}
