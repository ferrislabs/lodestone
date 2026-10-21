mod support;

use api::openapi::ApiDoc;
use axum::Router;
use http::{Method, StatusCode, header};
use httpmock::{
    Method::{GET, POST},
    MockServer,
};
use serde_json::{Value, from_str, json, to_value};
use sqlx::PgPool;
use support::{Harness, send, send_raw};
use utoipa::OpenApi;

const SECRET: &str = "fts-secret-0123456789-abcd";
const BASE: &str = "/api/v1/source-providers";

struct Api {
    harness: Harness,
    upstream: MockServer,
    bearer: String,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self {
            harness,
            upstream: MockServer::start_async().await,
            bearer,
        }
    }

    fn app(&self) -> Router {
        self.harness.public()
    }

    async fn call(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, json, _) = send(self.app(), method, uri, Some(&self.bearer), body).await;
        (status, json)
    }

    async fn raw(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, String) {
        let (status, _, raw) = send(self.app(), method, uri, Some(&self.bearer), body).await;
        (status, raw)
    }

    fn boamp_body(&self) -> Value {
        json!({"settings": {"base_url": self.upstream.url("/records")}})
    }

    fn france_travail_body(&self, secret: Option<&str>) -> Value {
        let secrets = match secret {
            Some(value) => json!({"client_secret": value}),
            None => json!({}),
        };
        json!({
            "settings": {
                "client_id": "client-1",
                "token_url": self.upstream.url("/token"),
                "api_base_url": self.upstream.url("/api"),
            },
            "secrets": secrets,
        })
    }

    async fn upstream_ok(&self) {
        self.upstream
            .mock_async(|when, then| {
                when.method(POST).path("/token");
                then.status(200)
                    .json_body(json!({"access_token": "tok-1", "expires_in": 900}));
            })
            .await;
        self.upstream
            .mock_async(|when, then| {
                when.method(GET).path("/api/offres/search");
                then.status(206).body("{}");
            })
            .await;
    }
}

fn secret(data: &Value) -> &Value {
    data["secrets"]
        .as_array()
        .expect("secrets")
        .iter()
        .find(|secret| secret["key"] == "client_secret")
        .expect("client_secret")
}

fn paths(body: &Value) -> Vec<String> {
    body["details"]
        .as_array()
        .expect("details")
        .iter()
        .map(|detail| detail["path"].as_str().expect("path").to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_endpoint_but_the_callback_requires_a_user(pool: PgPool) {
    let api = Api::new(pool).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let calls = [
        (Method::GET, BASE.to_owned()),
        (Method::GET, format!("{BASE}/boamp")),
        (Method::PUT, format!("{BASE}/boamp")),
        (Method::POST, format!("{BASE}/boamp/test")),
        (Method::POST, format!("{BASE}/boamp/oauth/start")),
        (Method::POST, format!("{BASE}/boamp/disconnect")),
        (Method::DELETE, format!("{BASE}/boamp")),
    ];

    for (method, uri) in calls {
        let body = (method == Method::PUT).then(|| json!({}));
        let (anonymous, _, _) = send(api.app(), method.clone(), &uri, None, body.clone()).await;
        assert_eq!(anonymous, StatusCode::UNAUTHORIZED, "{method} {uri}");
        let (service, _, _) = send(api.app(), method.clone(), &uri, Some(&client), body).await;
        assert_eq!(service, StatusCode::FORBIDDEN, "{method} {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_catalog_lists_every_provider_as_not_configured(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, body) = api.call(Method::GET, BASE, None).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let providers = body["data"].as_array().expect("array");
    let keys: Vec<&str> = providers
        .iter()
        .map(|provider| provider["key"].as_str().expect("key"))
        .collect();
    assert_eq!(
        keys,
        vec![
            "boamp",
            "france_travail",
            "decp",
            "ted",
            "ats",
            "hacker_news_hiring",
            "certificate_transparency",
            "dns_cname",
            "http_probe",
            "contacts",
            "company_research",
            "bodacc"
        ]
    );
    for provider in providers {
        assert_eq!(provider["status"], "not_configured");
    }
    assert_eq!(providers[0]["category"], "public_procurement");
    assert_eq!(providers[0]["auth_kind"], "none");
    assert_eq!(providers[1]["auth_kind"], "oauth2_client_credentials");
    assert_eq!(providers[1]["capabilities"]["keyword_search"], true);
    assert_eq!(providers[1]["capabilities"]["filters"]["contract"], true);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_provider_detail_exposes_its_form_and_no_value_when_unconfigured(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, body) = api
        .call(Method::GET, &format!("{BASE}/france_travail"), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["summary"]["status"], "not_configured");
    let fields = data["fields"].as_array().expect("fields");
    let spec = fields
        .iter()
        .find(|field| field["key"] == "client_secret")
        .expect("secret field");
    assert_eq!(spec["kind"], "secret");
    assert_eq!(spec["required"], true);
    assert_eq!(secret(data)["is_set"], false);
    assert!(data["terms_note"].is_string());
    assert!(data["oauth_redirect_uri"].is_null());
    assert_eq!(data["settings"], json!({}));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_provider_is_not_found(pool: PgPool) {
    let api = Api::new(pool).await;
    for (method, uri) in [
        (Method::GET, format!("{BASE}/nope")),
        (Method::PUT, format!("{BASE}/nope")),
        (Method::POST, format!("{BASE}/nope/test")),
        (Method::DELETE, format!("{BASE}/nope")),
    ] {
        let body = (method == Method::PUT).then(|| json!({}));
        let (status, _) = api.call(method.clone(), &uri, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn putting_a_provider_without_auth_runs_the_test_and_makes_it_ready(pool: PgPool) {
    let api = Api::new(pool).await;
    let records = api
        .upstream
        .mock_async(|when, then| {
            when.method(GET).path("/records").query_param("limit", "1");
            then.status(200).json_body(json!({"results": []}));
        })
        .await;

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/boamp"),
            Some(api.boamp_body()),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["summary"]["status"], "ready");
    assert!(body["data"]["summary"]["last_test_at"].is_string());
    assert!(body["data"]["last_test_message"].is_string());
    records.assert_async().await;
    let (_, listed) = api.call(Method::GET, BASE, None).await;
    assert_eq!(listed["data"][0]["status"], "ready");
    assert_eq!(listed["data"][1]["status"], "not_configured");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failing_test_keeps_its_message_and_the_provider_unusable(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream
        .mock_async(|when, then| {
            when.method(GET).path("/records");
            then.status(500);
        })
        .await;

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/boamp"),
            Some(api.boamp_body()),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["summary"]["status"], "failing");
    assert!(
        body["data"]["last_test_message"]
            .as_str()
            .expect("message")
            .contains("500")
    );
    assert!(
        !api.harness
            .state
            .usecase
            .is_provider_usable("boamp")
            .await
            .unwrap()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn secrets_are_masked_in_every_response_and_kept_when_absent(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream_ok().await;

    let (status, raw) = api
        .raw(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(api.france_travail_body(Some(SECRET))),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(!raw.contains(SECRET));
    let body: Value = from_str(&raw).expect("json");
    assert_eq!(body["data"]["summary"]["status"], "ready");
    assert_eq!(secret(&body["data"])["is_set"], true);
    assert_eq!(secret(&body["data"])["hint"], "••••abcd");

    let (status, raw) = api
        .raw(Method::GET, &format!("{BASE}/france_travail"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!raw.contains(SECRET));
    assert!(raw.contains("••••abcd"));

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(api.france_travail_body(None)),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["summary"]["status"], "ready");
    assert_eq!(secret(&body["data"])["hint"], "••••abcd");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rejected_service_account_is_reported_failing_without_leaking(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(401);
        })
        .await;

    let (status, raw) = api
        .raw(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(api.france_travail_body(Some(SECRET))),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{raw}");
    assert!(!raw.contains(SECRET));
    let body: Value = from_str(&raw).expect("json");
    assert_eq!(body["data"]["summary"]["status"], "failing");
    assert!(
        body["data"]["last_test_message"]
            .as_str()
            .expect("message")
            .contains("401")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_configurations_are_rejected_with_their_paths(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(json!({"settings": {"token_url": "ftp://x", "other": 1}})),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let found = paths(&body);
    for expected in ["client_id", "token_url", "other", "client_secret"] {
        assert!(
            found.iter().any(|path| path == expected),
            "{expected} in {found:?}"
        );
    }

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(json!({"settings": {"client_id": "a"}, "secrets": {"client_secret": 12}})),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), vec!["secrets.client_secret"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_test_endpoint_retests_and_requires_a_configuration(pool: PgPool) {
    let api = Api::new(pool).await;
    let (status, _) = api
        .call(Method::POST, &format!("{BASE}/boamp/test"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let failing = api
        .upstream
        .mock_async(|when, then| {
            when.method(GET).path("/records");
            then.status(503);
        })
        .await;
    let (_, saved) = api
        .call(
            Method::PUT,
            &format!("{BASE}/boamp"),
            Some(api.boamp_body()),
        )
        .await;
    assert_eq!(saved["data"]["summary"]["status"], "failing");
    failing.delete_async().await;
    api.upstream
        .mock_async(|when, then| {
            when.method(GET).path("/records");
            then.status(200).json_body(json!({}));
        })
        .await;

    let (status, body) = api
        .call(Method::POST, &format!("{BASE}/boamp/test"), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["summary"]["status"], "ready");
}

#[sqlx::test(migrations = "../../migrations")]
async fn deleting_clears_the_configuration(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream_ok().await;
    api.call(
        Method::PUT,
        &format!("{BASE}/france_travail"),
        Some(api.france_travail_body(Some(SECRET))),
    )
    .await;

    let (status, _) = api
        .call(Method::DELETE, &format!("{BASE}/france_travail"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = api
        .call(Method::GET, &format!("{BASE}/france_travail"), None)
        .await;
    assert_eq!(body["data"]["summary"]["status"], "not_configured");
    assert_eq!(secret(&body["data"])["is_set"], false);
    let (again, _) = api
        .call(Method::DELETE, &format!("{BASE}/france_travail"), None)
        .await;
    assert_eq!(again, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn oauth_endpoints_refuse_providers_without_the_authorization_code_mode(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream_ok().await;
    api.call(
        Method::PUT,
        &format!("{BASE}/france_travail"),
        Some(api.france_travail_body(Some(SECRET))),
    )
    .await;

    for uri in [
        format!("{BASE}/france_travail/oauth/start"),
        format!("{BASE}/france_travail/disconnect"),
    ] {
        let (status, _) = api.call(Method::POST, &uri, None).await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_callback_is_public_and_redirects_to_the_webapp_with_an_error_for_a_bad_state(
    pool: PgPool,
) {
    let api = Api::new(pool).await;

    let (status, headers, _) = send_raw(
        api.app(),
        Method::GET,
        &format!("{BASE}/oauth/callback?code=abc&state=forged"),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        headers[header::LOCATION],
        "http://localhost:3000/sources?error=invalid_state"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_callback_reports_a_denied_consent_and_missing_parameters(pool: PgPool) {
    let api = Api::new(pool).await;

    let (_, denied, _) = send_raw(
        api.app(),
        Method::GET,
        &format!("{BASE}/oauth/callback?error=access_denied&state=unknown"),
        None,
        None,
    )
    .await;
    assert_eq!(
        denied[header::LOCATION],
        "http://localhost:3000/sources?error=denied"
    );

    let (_, missing, _) = send_raw(
        api.app(),
        Method::GET,
        &format!("{BASE}/oauth/callback"),
        None,
        None,
    )
    .await;
    assert_eq!(
        missing[header::LOCATION],
        "http://localhost:3000/sources?error=invalid_request"
    );
}

#[test]
fn the_openapi_document_describes_every_endpoint() {
    let document = to_value(ApiDoc::openapi()).expect("document");
    let paths = &document["paths"];
    for (path, method) in [
        ("/api/v1/source-providers", "get"),
        ("/api/v1/source-providers/{key}", "get"),
        ("/api/v1/source-providers/{key}", "put"),
        ("/api/v1/source-providers/{key}", "delete"),
        ("/api/v1/source-providers/{key}/test", "post"),
        ("/api/v1/source-providers/{key}/oauth/start", "post"),
        ("/api/v1/source-providers/oauth/callback", "get"),
        ("/api/v1/source-providers/{key}/disconnect", "post"),
    ] {
        assert!(paths[path][method].is_object(), "{method} {path}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn moving_a_url_without_the_secrets_is_refused_and_nothing_is_sent(pool: PgPool) {
    let api = Api::new(pool).await;
    api.upstream_ok().await;
    api.call(
        Method::PUT,
        &format!("{BASE}/france_travail"),
        Some(api.france_travail_body(Some(SECRET))),
    )
    .await;
    let thief = MockServer::start_async().await;
    let token = thief
        .mock_async(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .json_body(json!({"access_token": "t", "expires_in": 60}));
        })
        .await;
    let mut moved = api.france_travail_body(None);
    moved["settings"]["token_url"] = json!(thief.url("/token"));

    let (status, body) = api
        .call(
            Method::PUT,
            &format!("{BASE}/france_travail"),
            Some(moved.clone()),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(paths(&body), vec!["secrets.client_secret"]);
    token.assert_hits_async(0).await;

    moved["secrets"] = json!({"client_secret": SECRET});
    let (status, body) = api
        .call(Method::PUT, &format!("{BASE}/france_travail"), Some(moved))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    token.assert_hits_async(1).await;
}
