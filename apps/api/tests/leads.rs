mod support;

use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, call, lazy_pool, send};
use utoipa::OpenApi;

struct Seed {
    pool: PgPool,
}

impl Seed {
    async fn profile(&self, name: &str) -> String {
        let id = common::generate_uuid_v7();
        sqlx::query(
            "INSERT INTO profiles (id, name, active, created_at, updated_at) VALUES ($1, $2, true, now(), now())",
        )
        .bind(id)
        .bind(name)
        .execute(&self.pool)
        .await
        .expect("profile");
        sqlx::query(
            "INSERT INTO profile_revisions (profile_id, revision, definition, created_at) VALUES ($1, 1, '{}'::jsonb, now()), ($1, 2, '{}'::jsonb, now())",
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .expect("revisions");
        id.to_string()
    }

    async fn lead(
        &self,
        profile: &str,
        domain: &str,
        status: &str,
        score: f64,
        seen_day: i32,
    ) -> String {
        let id = common::generate_uuid_v7();
        sqlx::query(
            "INSERT INTO leads (id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, $6, now() - make_interval(days => $7 + 10), now() - make_interval(days => $7), now())",
        )
        .bind(id)
        .bind(profile)
        .bind(domain)
        .bind(status)
        .bind(score)
        .bind(json!([{"signal": "product_detected", "value": "keycloak", "weight": 10.0, "measure": 0.8, "contribution": 8.0}]))
        .bind(seen_day)
        .execute(&self.pool)
        .await
        .expect("lead");
        id.to_string()
    }

    async fn signal(&self, lead: &str, kind: &str) {
        sqlx::query(
            "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint)
             VALUES ($1, $2::uuid, $3, 'keycloak', 'http_probe', '{\"status\": 200}'::jsonb, 'https://example.com/realms/master', 0.8, now(), $1::text)",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(kind)
        .execute(&self.pool)
        .await
        .expect("signal");
    }

    async fn contact(&self, lead: &str, address: &str) {
        sqlx::query(
            "INSERT INTO contact_emails (id, lead_id, address, provenance, method, confidence, kind, found_at)
             VALUES ($1, $2::uuid, $3, 'https://example.com/security.txt', 'security_txt', 0.9, 'role', now())",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(address)
        .execute(&self.pool)
        .await
        .expect("contact");
    }

    async fn assessment(&self, lead: &str) {
        sqlx::query(
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body)
             VALUES ($1, $2::uuid, 2, 'Runs Keycloak', 'Offer managed IAM', $3, 'model-x', 'qualification-v1', now(), 'Hello', 'Body text')",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(json!([{"quote": "Powered by Keycloak", "provenance": "https://example.com/about"}]))
        .execute(&self.pool)
        .await
        .expect("assessment");
    }

    async fn suppress(&self, domain: &str) {
        sqlx::query(
            "INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, 'domain', $2, now())",
        )
        .bind(common::generate_uuid_v7())
        .bind(domain)
        .execute(&self.pool)
        .await
        .expect("suppression");
    }
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

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let (status, body, _) = call(self.app(), uri, Some(&self.bearer)).await;
        (status, body)
    }

    async fn patch_status(&self, lead: &str, status: &str) -> (StatusCode, Value) {
        let (status, body, _) = send(
            self.app(),
            Method::PATCH,
            &format!("/api/v1/leads/{lead}/status"),
            Some(&self.bearer),
            Some(json!({ "status": status })),
        )
        .await;
        (status, body)
    }
}

fn domains(body: &Value) -> Vec<&str> {
    body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|lead| lead["domain"].as_str().expect("domain"))
        .collect()
}

async fn listing_fixture(pool: &PgPool) -> (Seed, String) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let a = seed.lead(&profile, "alpha.com", "new", 10.0, 3).await;
    seed.lead(&profile, "bravo.com", "qualified", 40.0, 2).await;
    seed.lead(&profile, "charlie.com", "new", 25.0, 1).await;
    seed.signal(&a, "product_detected").await;
    (seed, profile)
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_are_listed_with_revision_and_visible_lead_count(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    seed.lead(&profile, "optout.io", "new", 5.0, 1).await;
    seed.suppress("optout.io").await;
    let api = Api::new(pool).await;

    let (status, body) = api.get("/api/v1/profiles").await;

    assert_eq!(status, StatusCode::OK);
    let item = &body["data"][0];
    assert_eq!(item["id"], profile);
    assert_eq!(item["name"], "keycloak-ops");
    assert_eq!(item["active"], true);
    assert_eq!(item["revision"], 2);
    assert_eq!(item["lead_count"], 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_are_listed_by_score_with_their_profile_and_pagination(pool: PgPool) {
    let (_seed, profile) = listing_fixture(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), ["bravo.com", "charlie.com", "alpha.com"]);
    let first = &body["data"][0];
    assert_eq!(first["profile"]["id"], profile);
    assert_eq!(first["profile"]["name"], "keycloak-ops");
    assert_eq!(first["status"], "qualified");
    assert_eq!(first["score"], 40.0);
    assert_eq!(body["pagination"]["total"], 3);
    assert_eq!(body["pagination"]["per_page"], 20);
    assert_eq!(body["pagination"]["current_page"], 1);
    assert_eq!(body["pagination"]["last_page"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_filter_narrows_the_list(pool: PgPool) {
    let (_seed, profile) = listing_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, by_status) = api.get(&format!("{base}?status=new")).await;
    assert_eq!(domains(&by_status), ["charlie.com", "alpha.com"]);

    let (_, by_score) = api.get(&format!("{base}?min_score=25")).await;
    assert_eq!(domains(&by_score), ["bravo.com", "charlie.com"]);

    let (_, by_domain) = api.get(&format!("{base}?domain=HARL")).await;
    assert_eq!(domains(&by_domain), ["charlie.com"]);
    assert_eq!(by_domain["pagination"]["total"], 1);

    let (_, by_signal) = api.get(&format!("{base}?signal=product_detected")).await;
    assert_eq!(domains(&by_signal), ["alpha.com"]);

    let (_, combined) = api.get(&format!("{base}?status=new&min_score=20")).await;
    assert_eq!(domains(&combined), ["charlie.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_sort_orders_the_list(pool: PgPool) {
    let (_seed, profile) = listing_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, score_asc) = api.get(&format!("{base}?sort=score_asc")).await;
    assert_eq!(
        domains(&score_asc),
        ["alpha.com", "charlie.com", "bravo.com"]
    );

    let (_, last_seen) = api.get(&format!("{base}?sort=last_seen_desc")).await;
    assert_eq!(
        domains(&last_seen),
        ["charlie.com", "bravo.com", "alpha.com"]
    );

    let (_, first_seen) = api.get(&format!("{base}?sort=first_seen_desc")).await;
    assert_eq!(
        domains(&first_seen),
        ["charlie.com", "bravo.com", "alpha.com"]
    );

    let (_, by_domain) = api.get(&format!("{base}?sort=domain_asc")).await;
    assert_eq!(
        domains(&by_domain),
        ["alpha.com", "bravo.com", "charlie.com"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn pagination_metadata_follows_page_and_per_page(pool: PgPool) {
    let (_seed, profile) = listing_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, second) = api.get(&format!("{base}?per_page=2&page=2")).await;

    assert_eq!(domains(&second), ["alpha.com"]);
    assert_eq!(second["pagination"]["total"], 3);
    assert_eq!(second["pagination"]["per_page"], 2);
    assert_eq!(second["pagination"]["current_page"], 2);
    assert_eq!(second["pagination"]["last_page"], 2);
    assert_eq!(second["pagination"]["prev_page"], 1);
    assert!(second["pagination"].get("next_page").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_query_parameters_are_rejected_naming_the_field(pool: PgPool) {
    let (_seed, profile) = listing_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    for (query, field) in [
        ("status=archived", "status"),
        ("per_page=101", "per_page"),
        ("per_page=0", "per_page"),
        ("page=0", "page"),
        ("page=abc", "page"),
        ("min_score=-1", "min_score"),
        ("min_score=abc", "min_score"),
        ("sort=random", "sort"),
        ("signal=Not-Valid", "signal"),
    ] {
        let (status, body) = api.get(&format!("{base}?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert!(
            body["message"].as_str().is_some_and(|m| m.contains(field)),
            "{query}: {body}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_of_another_profile_are_not_listed(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let other = seed.profile("other").await;
    seed.lead(&other, "elsewhere.com", "new", 99.0, 1).await;
    let api = Api::new(pool).await;

    let (_, mine) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;
    let (_, theirs) = api.get(&format!("/api/v1/profiles/{other}/leads")).await;

    assert!(!domains(&mine).contains(&"elsewhere.com"));
    assert_eq!(domains(&theirs), ["elsewhere.com"]);
    assert_eq!(theirs["data"][0]["profile"]["name"], "other");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_profile_is_not_found_and_malformed_id_is_rejected(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, _) = api
        .get(&format!(
            "/api/v1/profiles/{}/leads",
            common::generate_uuid_v7()
        ))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = api.get("/api/v1/profiles/not-a-uuid/leads").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_are_hidden_from_the_list_and_not_found_in_detail(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let hidden = seed.lead(&profile, "optout.io", "new", 90.0, 1).await;
    seed.suppress("optout.io").await;
    let api = Api::new(pool).await;

    let (_, list) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;
    let (status, _) = api.get(&format!("/api/v1/leads/{hidden}")).await;
    let (patch_status, _) = api.patch_status(&hidden, "qualified").await;

    assert!(!domains(&list).contains(&"optout.io"));
    assert_eq!(list["pagination"]["total"], 3);
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(patch_status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_carries_profile_signals_contacts_assessments_and_breakdown(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "detail.com", "qualified", 8.0, 1).await;
    seed.signal(&lead, "product_detected").await;
    seed.contact(&lead, "security@detail.com").await;
    seed.assessment(&lead).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["lead"]["id"], lead);
    assert_eq!(data["lead"]["domain"], "detail.com");
    assert_eq!(data["lead"]["status"], "qualified");
    assert_eq!(data["lead"]["profile"]["id"], profile);
    assert_eq!(data["lead"]["profile"]["name"], "keycloak-ops");

    assert_eq!(data["score_breakdown"][0]["signal"], "product_detected");
    assert_eq!(data["score_breakdown"][0]["value"], "keycloak");
    assert_eq!(data["score_breakdown"][0]["contribution"], 8.0);

    let signal = &data["signals"][0];
    assert_eq!(signal["kind"], "product_detected");
    assert_eq!(signal["source"], "http_probe");
    assert_eq!(signal["provenance"], "https://example.com/realms/master");
    assert_eq!(signal["strength"], 0.8);
    assert_eq!(signal["raw"]["status"], 200);
    assert!(signal["observed_at"].is_string());

    let contact = &data["contacts"][0];
    assert_eq!(contact["address"], "security@detail.com");
    assert_eq!(contact["method"], "security_txt");
    assert_eq!(contact["confidence"], 0.9);
    assert_eq!(contact["provenance"], "https://example.com/security.txt");

    let assessment = &data["assessments"][0];
    assert_eq!(assessment["summary"], "Runs Keycloak");
    assert_eq!(assessment["approach_angle"], "Offer managed IAM");
    assert_eq!(assessment["model"], "model-x");
    assert_eq!(assessment["prompt_version"], "qualification-v1");
    assert_eq!(assessment["profile_revision"], 2);
    assert_eq!(assessment["evidence"][0]["quote"], "Powered by Keycloak");
    assert_eq!(
        assessment["evidence"][0]["provenance"],
        "https://example.com/about"
    );
    assert_eq!(assessment["draft"]["subject"], "Hello");
    assert_eq!(assessment["draft"]["body"], "Body text");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_or_malformed_lead_ids_are_not_served(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, _) = api
        .get(&format!("/api/v1/leads/{}", common::generate_uuid_v7()))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = api.get("/api/v1/leads/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_valid_transition_updates_the_lead(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "move.com", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (status, body) = api.patch_status(&lead, "qualified").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["id"], lead);
    assert_eq!(body["data"]["status"], "qualified");
    assert_eq!(body["data"]["profile"]["name"], "keycloak-ops");
    let (_, detail) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert_eq!(detail["data"]["lead"]["status"], "qualified");
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_transition_conflicts_and_leaves_the_lead_untouched(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "stuck.com", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (status, body) = api.patch_status(&lead, "contacted").await;

    assert_eq!(status, StatusCode::CONFLICT);
    let message = body["message"].as_str().expect("message");
    assert!(
        message.contains("new") && message.contains("contacted"),
        "{message}"
    );
    let (_, detail) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert_eq!(detail["data"]["lead"]["status"], "new");
}

#[sqlx::test(migrations = "../../migrations")]
async fn status_changes_reject_unknown_statuses_and_unknown_leads(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "move.com", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (status, body) = api.patch_status(&lead, "archived").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("status"))
    );

    let (status, _) = api
        .patch_status(&common::generate_uuid_v7().to_string(), "qualified")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn every_leads_route_requires_authentication() {
    let harness = Harness::new(lazy_pool()).await;
    let id = common::generate_uuid_v7();

    for (method, uri) in [
        (Method::GET, "/api/v1/profiles".to_owned()),
        (Method::GET, format!("/api/v1/profiles/{id}/leads")),
        (Method::GET, format!("/api/v1/leads/{id}")),
        (Method::PATCH, format!("/api/v1/leads/{id}/status")),
    ] {
        let (status, body, _) = send(
            harness.public(),
            method.clone(),
            &uri,
            None,
            Some(json!({ "status": "qualified" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert_eq!(body["code"], "E_MISSING_AUTH_HEADER");
    }
}

#[test]
fn openapi_documents_the_leads_paths() {
    let spec = api::openapi::ApiDoc::openapi();

    for (path, method) in [
        ("/api/v1/profiles", "get"),
        ("/api/v1/profiles/{profile_id}/leads", "get"),
        ("/api/v1/leads/{lead_id}", "get"),
        ("/api/v1/leads/{lead_id}/status", "patch"),
    ] {
        let item = spec
            .paths
            .paths
            .get(path)
            .unwrap_or_else(|| panic!("{path}"));
        let json = serde_json::to_value(item).expect("path item");
        assert!(json.get(method).is_some(), "{method} {path}");
    }
}
