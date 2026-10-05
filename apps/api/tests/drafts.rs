mod support;

use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, send};
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
        id.to_string()
    }

    async fn lead(&self, profile: &str, domain: &str, status: &str, score: f64) -> String {
        let id = common::generate_uuid_v7();
        sqlx::query(
            "INSERT INTO leads (id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, '[]'::jsonb, now(), now(), now())",
        )
        .bind(id)
        .bind(profile)
        .bind(domain)
        .bind(status)
        .bind(score)
        .execute(&self.pool)
        .await
        .expect("lead");
        id.to_string()
    }

    async fn draft(&self, lead: &str, age_seconds: i64, with_draft: bool) -> String {
        let id = common::generate_uuid_v7();
        sqlx::query(
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status)
             VALUES ($1, $2::uuid, 1, 'Runs Keycloak', 'Offer managed IAM', $3, 'model-x', 'qualification@1', now() - make_interval(secs => $4), $5, $6, CASE WHEN $5::text IS NULL THEN NULL ELSE 'pending' END)",
        )
        .bind(id)
        .bind(lead)
        .bind(json!([{"quote": "Powered by Keycloak", "provenance": "https://example.com/about"}]))
        .bind(age_seconds as f64)
        .bind(with_draft.then_some("Hello"))
        .bind(with_draft.then_some("Body text"))
        .execute(&self.pool)
        .await
        .expect("assessment");
        id.to_string()
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

    async fn lead_status(&self, lead: &str) -> String {
        sqlx::query_scalar("SELECT status FROM leads WHERE id = $1::uuid")
            .bind(lead)
            .fetch_one(&self.pool)
            .await
            .expect("lead status")
    }

    async fn count(&self, table: &str) -> i64 {
        sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&self.pool)
            .await
            .expect("count")
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

    async fn request(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, body, _) = send(self.app(), method, uri, Some(&self.bearer), body).await;
        (status, body)
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.request(Method::GET, uri, None).await
    }

    async fn post(&self, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.request(Method::POST, uri, body).await
    }

    async fn patch(&self, id: &str, body: Value) -> (StatusCode, Value) {
        self.request(Method::PATCH, &format!("/api/v1/drafts/{id}"), Some(body))
            .await
    }
}

fn domains(body: &Value) -> Vec<&str> {
    body["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|item| item["lead"]["domain"].as_str().expect("domain"))
        .collect()
}

fn paths(body: &Value) -> Vec<&str> {
    body["details"]
        .as_array()
        .expect("details")
        .iter()
        .map(|detail| detail["path"].as_str().expect("path"))
        .collect()
}

async fn single(pool: &PgPool) -> (Seed, String, String) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "alpha.com", "qualified", 10.0).await;
    let draft = seed.draft(&lead, 10, true).await;
    (seed, lead, draft)
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_queue_lists_only_latest_pending_drafts_of_qualified_visible_leads(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let other = seed.profile("other").await;

    let high = seed.lead(&profile, "high.com", "qualified", 9.0).await;
    seed.draft(&high, 10, true).await;
    seed.contact(&high, "security@high.com").await;
    let low = seed.lead(&profile, "low.com", "qualified", 2.0).await;
    seed.draft(&low, 10, true).await;
    let elsewhere = seed.lead(&other, "elsewhere.com", "qualified", 5.0).await;
    seed.draft(&elsewhere, 10, true).await;

    let superseded = seed
        .lead(&profile, "superseded.com", "qualified", 8.0)
        .await;
    seed.draft(&superseded, 100, true).await;
    seed.draft(&superseded, 10, false).await;
    let validated = seed.lead(&profile, "validated.com", "validated", 8.0).await;
    seed.draft(&validated, 10, true).await;
    let hidden = seed.lead(&profile, "optout.com", "qualified", 8.0).await;
    seed.draft(&hidden, 10, true).await;
    seed.suppress("optout.com").await;
    let decided = seed.lead(&profile, "decided.com", "qualified", 8.0).await;
    let decided_draft = seed.draft(&decided, 10, true).await;
    sqlx::query("UPDATE assessments SET draft_status = 'rejected', draft_reviewed_at = now() WHERE id = $1::uuid")
        .bind(&decided_draft)
        .execute(&pool)
        .await
        .expect("decide");
    let api = Api::new(pool).await;

    let (status, body) = api.get("/api/v1/drafts").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), ["high.com", "elsewhere.com", "low.com"]);
    assert_eq!(body["pagination"]["total"], 3);
    let first = &body["data"][0];
    assert_eq!(first["lead"]["profile"]["id"], profile);
    assert_eq!(first["lead"]["profile"]["name"], "keycloak-ops");
    assert_eq!(first["lead"]["score"], 9.0);
    assert_eq!(first["draft"]["subject"], "Hello");
    assert_eq!(first["draft"]["status"], "pending");
    assert_eq!(first["draft"]["original_subject"], Value::Null);
    assert_eq!(first["summary"], "Runs Keycloak");
    assert_eq!(first["approach_angle"], "Offer managed IAM");
    assert_eq!(first["evidence"][0]["quote"], "Powered by Keycloak");
    assert_eq!(first["contact"]["address"], "security@high.com");
    assert_eq!(first["contact"]["method"], "security_txt");
    assert_eq!(body["data"][1]["contact"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_queue_filters_by_profile_and_paginates(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let other = seed.profile("other").await;
    for (domain, score) in [("a.com", 3.0), ("b.com", 2.0), ("c.com", 1.0)] {
        let lead = seed.lead(&profile, domain, "qualified", score).await;
        seed.draft(&lead, 10, true).await;
    }
    let elsewhere = seed.lead(&other, "z.com", "qualified", 50.0).await;
    seed.draft(&elsewhere, 10, true).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .get(&format!(
            "/api/v1/drafts?profile_id={profile}&page=2&per_page=2"
        ))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), ["c.com"]);
    assert_eq!(body["pagination"]["total"], 3);
    assert_eq!(body["pagination"]["current_page"], 2);
    assert_eq!(body["pagination"]["per_page"], 2);

    let (_, body) = api.get(&format!("/api/v1/drafts?profile_id={other}")).await;
    assert_eq!(domains(&body), ["z.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_parameters_are_validated(pool: PgPool) {
    let api = Api::new(pool).await;

    for uri in [
        "/api/v1/drafts?profile_id=nope",
        "/api/v1/drafts?page=0",
        "/api/v1/drafts?per_page=101",
        "/api/v1/drafts?page=abc",
    ] {
        assert_eq!(api.get(uri).await.0, StatusCode::BAD_REQUEST, "{uri}");
    }
    let unknown = format!("/api/v1/drafts?profile_id={}", common::generate_uuid_v7());
    assert_eq!(api.get(&unknown).await.0, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_draft_is_read_by_assessment_id(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let no_draft = seed.draft(&lead, 1, false).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/drafts/{draft}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["assessment_id"], draft);
    assert_eq!(body["data"]["lead"]["id"], lead);

    assert_eq!(
        api.get(&format!("/api/v1/drafts/{no_draft}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.get(&format!("/api/v1/drafts/{}", common::generate_uuid_v7()))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.get("/api/v1/drafts/not-a-uuid").await.0,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_keeps_the_original_and_stamps_the_edit(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .patch(&draft, json!({"subject": "Better subject"}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body_after) = api.patch(&draft, json!({"body": "Better body"})).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(body["data"]["draft"]["subject"], "Better subject");
    assert_eq!(body["data"]["draft"]["body"], "Body text");
    assert_eq!(body_after["data"]["draft"]["subject"], "Better subject");
    assert_eq!(body_after["data"]["draft"]["body"], "Better body");
    assert_eq!(body_after["data"]["draft"]["original_subject"], "Hello");
    assert_eq!(body_after["data"]["draft"]["original_body"], "Body text");
    assert!(body_after["data"]["draft"]["edited_at"].is_string());
    assert_eq!(body_after["data"]["draft"]["status"], "pending");

    let (_, listed) = api.get("/api/v1/drafts").await;
    assert_eq!(listed["data"][0]["draft"]["subject"], "Better subject");
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_edits_are_rejected_with_field_paths(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .patch(&draft, json!({"subject": "", "body": "   "}))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), ["subject", "body"]);

    let (status, body) = api
        .patch(
            &draft,
            json!({"subject": "s".repeat(201), "body": "b".repeat(10_001)}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), ["subject", "body"]);

    let (status, body) = api.patch(&draft, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), ["$"]);

    let (status, _) = api
        .patch(
            &draft,
            json!({"subject": "s".repeat(200), "body": "b".repeat(10_000)}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_marks_the_draft_ready_and_validates_the_lead_without_sending(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let jobs_before = seed.count("jobs").await;
    let contacts_before = seed.count("contact_emails").await;
    let api = Api::new(pool.clone()).await;

    let (status, body) = api
        .post(&format!("/api/v1/drafts/{draft}/approve"), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["status"], "approved");
    assert!(body["data"]["draft"]["reviewed_at"].is_string());
    assert_eq!(seed.lead_status(&lead).await, "validated");
    let reviewer: Option<String> = sqlx::query_scalar(
        "SELECT u.sub FROM assessments a JOIN users u ON u.id = a.draft_reviewed_by WHERE a.id = $1::uuid",
    )
    .bind(&draft)
    .fetch_one(&pool)
    .await
    .expect("reviewer");
    assert_eq!(reviewer.as_deref(), Some("subject-42"));
    assert!(body["data"]["draft"]["reviewed_by"].is_string());
    assert_eq!(seed.count("jobs").await, jobs_before);
    assert_eq!(seed.count("jobs").await, 0);
    assert_eq!(seed.count("contact_emails").await, contacts_before);
    let (subject, text): (String, String) =
        sqlx::query_as("SELECT draft_subject, draft_body FROM assessments WHERE id = $1::uuid")
            .bind(&draft)
            .fetch_one(&pool)
            .await
            .expect("draft");
    assert_eq!((subject.as_str(), text.as_str()), ("Hello", "Body text"));

    let (_, listed) = api.get("/api/v1/drafts").await;
    assert_eq!(listed["data"].as_array().expect("data").len(), 0);
    let (_, detail) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert_eq!(
        detail["data"]["assessments"][0]["draft"]["status"],
        "approved"
    );
    assert_eq!(detail["data"]["lead"]["status"], "validated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_records_the_reason_and_rejects_the_lead(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .post(
            &format!("/api/v1/drafts/{draft}/reject"),
            Some(json!({"reason": "  Not a fit  "})),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["status"], "rejected");
    assert_eq!(body["data"]["draft"]["rejection_reason"], "Not a fit");
    assert_eq!(seed.lead_status(&lead).await, "rejected");
    assert_eq!(seed.count("jobs").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_without_a_body_is_allowed_and_a_long_reason_is_not(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let uri = format!("/api/v1/drafts/{draft}/reject");

    let (status, body) = api
        .post(&uri, Some(json!({"reason": "r".repeat(1001)})))
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(paths(&body), ["reason"]);
    assert_eq!(seed.lead_status(&lead).await, "qualified");

    let (status, body) = api.post(&uri, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["rejection_reason"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decided_draft_cannot_be_decided_or_edited_again(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let approve = format!("/api/v1/drafts/{draft}/approve");
    let reject = format!("/api/v1/drafts/{draft}/reject");

    assert_eq!(api.post(&approve, None).await.0, StatusCode::OK);

    assert_eq!(api.post(&approve, None).await.0, StatusCode::CONFLICT);
    assert_eq!(api.post(&reject, None).await.0, StatusCode::CONFLICT);
    assert_eq!(
        api.patch(&draft, json!({"subject": "late"})).await.0,
        StatusCode::CONFLICT
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_decisions_let_exactly_one_win(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let approve = format!("/api/v1/drafts/{draft}/approve");
    let reject = format!("/api/v1/drafts/{draft}/reject");

    let ((first, _), (second, _)) = tokio::join!(api.post(&approve, None), api.post(&reject, None));

    let mut statuses = [first, second];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    assert!(["validated", "rejected"].contains(&seed.lead_status(&lead).await.as_str()));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_that_left_qualified_cannot_be_decided(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    sqlx::query("UPDATE leads SET status = 'new' WHERE id = $1::uuid")
        .bind(&lead)
        .execute(&pool)
        .await
        .expect("move");
    let api = Api::new(pool).await;

    let (status, _) = api
        .post(&format!("/api/v1/drafts/{draft}/approve"), None)
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(seed.lead_status(&lead).await, "new");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_superseded_draft_cannot_be_decided(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    seed.draft(&lead, 1, true).await;
    let api = Api::new(pool).await;

    let (status, _) = api
        .post(&format!("/api/v1/drafts/{draft}/approve"), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(seed.lead_status(&lead).await, "qualified");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_lead_hides_its_draft_and_blocks_every_action(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    seed.suppress("alpha.com").await;
    let api = Api::new(pool).await;

    assert_eq!(
        api.get(&format!("/api/v1/drafts/{draft}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.patch(&draft, json!({"subject": "x"})).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.post(&format!("/api/v1/drafts/{draft}/approve"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        api.post(&format!("/api/v1/drafts/{draft}/reject"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, listed) = api.get("/api/v1/drafts").await;
    assert_eq!(listed["data"].as_array().expect("data").len(), 0);
    assert_eq!(seed.lead_status(&lead).await, "qualified");
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_route_requires_authentication(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let harness = Harness::new(pool).await;
    let routes = [
        (Method::GET, "/api/v1/drafts".to_owned()),
        (Method::GET, format!("/api/v1/drafts/{draft}")),
        (Method::PATCH, format!("/api/v1/drafts/{draft}")),
        (Method::POST, format!("/api/v1/drafts/{draft}/approve")),
        (Method::POST, format!("/api/v1/drafts/{draft}/reject")),
    ];

    for (method, uri) in routes {
        let (status, _, _) = send(harness.public(), method.clone(), &uri, None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_accounts_read_but_cannot_edit_or_decide(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let harness = Harness::new(pool).await;
    let bearer = format!("Bearer {}", harness.valid_client_token());

    let (status, _, _) = send(
        harness.public(),
        Method::GET,
        "/api/v1/drafts",
        Some(&bearer),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let writes = [
        (
            Method::PATCH,
            format!("/api/v1/drafts/{draft}"),
            Some(json!({"subject": "x"})),
        ),
        (
            Method::POST,
            format!("/api/v1/drafts/{draft}/approve"),
            None,
        ),
        (Method::POST, format!("/api/v1/drafts/{draft}/reject"), None),
    ];
    for (method, uri, body) in writes {
        let (status, _, _) =
            send(harness.public(), method.clone(), &uri, Some(&bearer), body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }
    assert_eq!(seed.lead_status(&lead).await, "qualified");
}

#[test]
fn openapi_documents_the_draft_paths() {
    let spec = api::openapi::ApiDoc::openapi();

    for (path, method) in [
        ("/api/v1/drafts", "get"),
        ("/api/v1/drafts/{assessment_id}", "get"),
        ("/api/v1/drafts/{assessment_id}", "patch"),
        ("/api/v1/drafts/{assessment_id}/approve", "post"),
        ("/api/v1/drafts/{assessment_id}/reject", "post"),
    ] {
        let value = serde_json::to_value(&spec).expect("spec");
        assert!(value["paths"][path][method].is_object(), "{method} {path}");
    }
}
