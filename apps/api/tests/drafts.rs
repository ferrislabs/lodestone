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
            "INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, now(), now(), now())",
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
    assert_eq!(first["draft"]["suggested_subject"], Value::Null);
    assert_eq!(first["draft"]["outdated_at"], Value::Null);
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
async fn the_queue_is_sorted_by_relevance_and_filtered_by_verdict(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    for (domain, score, judgement) in [
        ("waiting.com", 99.0, None),
        ("maybe.com", 1.0, Some((95.0, "a_verifier", 3.0))),
        ("sure.com", 1.0, Some((91.0, "fiable", 91.0))),
        ("out.com", 1.0, Some((5.0, "ecarte", 1.0))),
    ] {
        let lead = seed.lead(&profile, domain, "qualified", score).await;
        if let Some((confidence, verdict, reliable)) = judgement {
            sqlx::query("UPDATE leads SET confidence = $2, verdict = $3, reliable_probability = $4 WHERE id = $1::uuid")
                .bind(&lead)
                .bind(confidence)
                .bind(verdict)
                .bind(reliable)
                .execute(&pool)
                .await
                .unwrap();
        }
        seed.draft(&lead, 10, true).await;
    }
    sqlx::query("UPDATE assessments SET verdict_narrative = 'Les agents concordent.', evidence_verified = false")
        .execute(&pool)
        .await
        .unwrap();
    let api = Api::new(pool).await;

    let (_, all) = api.get("/api/v1/drafts").await;
    assert_eq!(
        domains(&all),
        ["sure.com", "maybe.com", "out.com", "waiting.com"]
    );
    let first = &all["data"][0];
    assert_eq!(first["lead"]["confidence"], 91.0);
    assert_eq!(first["lead"]["verdict"], "fiable");
    assert_eq!(first["verdict_narrative"], "Les agents concordent.");
    assert_eq!(first["evidence_verified"], false);
    assert!(all["data"][3]["lead"]["verdict"].is_null());

    let (_, visible) = api
        .get("/api/v1/drafts?verdict=fiable,a_verifier,en_attente")
        .await;
    assert_eq!(domains(&visible), ["sure.com", "maybe.com", "waiting.com"]);
    assert_eq!(visible["pagination"]["total"], 3);

    let (_, discarded) = api.get("/api/v1/drafts?verdict=ecarte").await;
    assert_eq!(domains(&discarded), ["out.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_kept_draft_exposes_the_newer_suggestion(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "qualified", 5.0).await;
    let draft = seed.draft(&lead, 10, true).await;
    sqlx::query("UPDATE assessments SET draft_edited_at = now(), draft_suggested_subject = 'Nouvel objet', draft_suggested_body = 'Nouveau corps', draft_outdated_at = now() WHERE id = $1::uuid")
        .bind(&draft)
        .execute(&pool)
        .await
        .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/drafts/{draft}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["suggested_subject"], "Nouvel objet");
    assert_eq!(body["data"]["draft"]["suggested_body"], "Nouveau corps");
    assert!(body["data"]["draft"]["outdated_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_parameters_are_validated(pool: PgPool) {
    let api = Api::new(pool).await;

    for uri in [
        "/api/v1/drafts?profile_id=nope",
        "/api/v1/drafts?page=0",
        "/api/v1/drafts?per_page=101",
        "/api/v1/drafts?page=abc",
        "/api/v1/drafts?verdict=maybe",
        "/api/v1/drafts?verdict=fiable%27%3B--",
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
            Some(json!({"reason": "autre", "note": "  Not a fit  "})),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["status"], "rejected");
    assert_eq!(body["data"]["draft"]["rejection_reason"], "Not a fit");
    assert_eq!(seed.lead_status(&lead).await, "rejected");
    assert_eq!(seed.count("jobs").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_requires_a_known_reason_and_a_bounded_note(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool.clone()).await;
    let uri = format!("/api/v1/drafts/{draft}/reject");

    for (body, path) in [
        (None, "reason"),
        (Some(json!({})), "reason"),
        (Some(json!({"reason": ""})), "reason"),
        (Some(json!({"reason": "pas_une_raison"})), "reason"),
        (Some(json!({"reason": "autre"})), "note"),
        (Some(json!({"reason": "autre", "note": "  "})), "note"),
        (
            Some(json!({"reason": "hors_cible", "note": "n".repeat(501)})),
            "note",
        ),
    ] {
        let (status, response) = api.post(&uri, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        assert_eq!(paths(&response), [path], "{body:?}");
    }
    assert_eq!(seed.lead_status(&lead).await, "qualified");
    assert_eq!(seed.count("decision_labels").await, 0);

    let (status, body) = api.post(&uri, Some(json!({"reason": "hors_cible"}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["rejection_reason"], Value::Null);
    let reasons: Vec<Option<String>> =
        sqlx::query_scalar("SELECT rejection_reason FROM decision_labels ORDER BY kind")
            .fetch_all(&pool)
            .await
            .expect("labels");
    assert_eq!(
        reasons,
        [Some("hors_cible".to_owned()), Some("hors_cible".to_owned())]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_decided_draft_cannot_be_decided_or_edited_again(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let approve = format!("/api/v1/drafts/{draft}/approve");
    let reject = format!("/api/v1/drafts/{draft}/reject");

    assert_eq!(api.post(&approve, None).await.0, StatusCode::OK);

    assert_eq!(api.post(&approve, None).await.0, StatusCode::CONFLICT);
    assert_eq!(
        api.post(&reject, Some(json!({"reason": "hors_cible"})))
            .await
            .0,
        StatusCode::CONFLICT
    );
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

    let ((first, _), (second, _)) = tokio::join!(
        api.post(&approve, None),
        api.post(&reject, Some(json!({"reason": "hors_cible"})))
    );

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
        api.post(
            &format!("/api/v1/drafts/{draft}/reject"),
            Some(json!({"reason": "hors_cible"}))
        )
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

async fn revision(api: &Api, draft: &str) -> u64 {
    let (status, body) = api.get(&format!("/api/v1/drafts/{draft}")).await;
    assert_eq!(status, StatusCode::OK);
    body["data"]["draft"]["revision"]
        .as_u64()
        .expect("revision")
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_draft_exposes_its_revision_and_each_edit_bumps_it(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    let (_, listed) = api.get("/api/v1/drafts").await;
    assert_eq!(listed["data"][0]["draft"]["revision"], 0);
    assert_eq!(revision(&api, &draft).await, 0);

    let (status, edited) = api.patch(&draft, json!({"subject": "One"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(edited["data"]["draft"]["revision"], 1);
    let (_, edited) = api.patch(&draft, json!({"body": "Two"})).await;
    assert_eq!(edited["data"]["draft"]["revision"], 2);

    let (_, listed) = api.get("/api/v1/drafts").await;
    assert_eq!(listed["data"][0]["draft"]["revision"], 2);
    let lead = listed["data"][0]["lead"]["id"].as_str().expect("lead");
    let (_, detail) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert_eq!(detail["data"]["assessments"][0]["draft"]["revision"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_edit_with_a_stale_revision_answers_conflict(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .patch(&draft, json!({"subject": "First", "expected_revision": 0}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["revision"], 1);

    let (status, body) = api
        .patch(&draft, json!({"subject": "Second", "expected_revision": 0}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.to_string().contains("draft changed"), "{body}");
    assert_eq!(revision(&api, &draft).await, 1);
    let (_, current) = api.get(&format!("/api/v1/drafts/{draft}")).await;
    assert_eq!(current["data"]["draft"]["subject"], "First");

    let (status, body) = api
        .patch(&draft, json!({"body": "Third", "expected_revision": 1}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["revision"], 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_with_a_stale_revision_answers_conflict_and_with_the_current_one_succeeds(
    pool: PgPool,
) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let approve = format!("/api/v1/drafts/{draft}/approve");
    api.patch(&draft, json!({"subject": "Edited"})).await;
    let current = revision(&api, &draft).await;
    assert_eq!(current, 1);

    let (status, _) = api
        .post(&approve, Some(json!({"expected_revision": current - 1})))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(seed.lead_status(&lead).await, "qualified");
    let (_, still) = api.get(&format!("/api/v1/drafts/{draft}")).await;
    assert_eq!(still["data"]["draft"]["status"], "pending");

    let (status, body) = api
        .post(&approve, Some(json!({"expected_revision": current})))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["status"], "approved");
    assert_eq!(seed.lead_status(&lead).await, "validated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn approving_without_a_body_or_with_an_empty_one_still_works(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let first = seed.lead(&profile, "first.com", "qualified", 2.0).await;
    let first_draft = seed.draft(&first, 10, true).await;
    let second = seed.lead(&profile, "second.com", "qualified", 1.0).await;
    let second_draft = seed.draft(&second, 10, true).await;
    let api = Api::new(pool).await;

    let (status, _) = api
        .post(&format!("/api/v1/drafts/{first_draft}/approve"), None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = api
        .post(
            &format!("/api/v1/drafts/{second_draft}/approve"),
            Some(json!({})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(seed.lead_status(&first).await, "validated");
    assert_eq!(seed.lead_status(&second).await, "validated");
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_with_a_stale_revision_answers_conflict(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;
    let reject = format!("/api/v1/drafts/{draft}/reject");
    api.patch(&draft, json!({"subject": "Edited"})).await;

    let (status, _) = api
        .post(
            &reject,
            Some(json!({"reason": "hors_cible", "expected_revision": 0})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(seed.lead_status(&lead).await, "qualified");

    let (status, body) = api
        .post(
            &reject,
            Some(json!({"reason": "hors_cible", "expected_revision": 1})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["draft"]["status"], "rejected");
    assert_eq!(seed.lead_status(&lead).await, "rejected");
}

#[sqlx::test(migrations = "../../migrations")]
async fn editing_a_superseded_draft_answers_conflict(pool: PgPool) {
    let (seed, lead, draft) = single(&pool).await;
    seed.draft(&lead, 1, true).await;
    let api = Api::new(pool).await;

    let (status, body) = api.patch(&draft, json!({"subject": "Late"})).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.to_string().contains("superseded"), "{body}");
    let (status, _) = api
        .patch(&draft, json!({"subject": "Late", "expected_revision": 0}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_expected_revision_is_a_bad_request(pool: PgPool) {
    let (_seed, _lead, draft) = single(&pool).await;
    let api = Api::new(pool).await;

    for value in [json!(-1), json!("one"), json!(1.5)] {
        let (status, _) = api
            .patch(&draft, json!({"subject": "x", "expected_revision": value}))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{value}");
    }
    assert_eq!(revision(&api, &draft).await, 0);
}

#[test]
fn openapi_documents_the_revision_guard() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).expect("spec");
    let schemas = &spec["components"]["schemas"];

    assert!(schemas["ApproveDraftRequest"]["properties"]["expected_revision"].is_object());
    assert!(schemas["EditDraftRequest"]["properties"]["expected_revision"].is_object());
    assert!(schemas["RejectDraftRequest"]["properties"]["expected_revision"].is_object());
    assert!(schemas["DraftDto"]["properties"]["revision"].is_object());
    assert!(
        spec["paths"]["/api/v1/drafts/{assessment_id}/approve"]["post"]["requestBody"].is_object()
    );
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
