mod support;

use http::{Method, StatusCode, header::CONTENT_DISPOSITION, header::CONTENT_TYPE};
use serde_json::Value;
use sqlx::PgPool;
use support::{Harness, call, send_raw};
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

    async fn assessment(&self, lead: &str, summary: &str) {
        sqlx::query(
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status, draft_reviewed_at)
             VALUES ($1, $2::uuid, 1, $3, 'Offer managed IAM', $4, 'model-x', 'qualification@1', now(), 'Hello', 'Body text', 'approved', now())",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(summary)
        .bind(serde_json::json!([{"quote": "Powered by Keycloak", "provenance": "https://example.com/about"}]))
        .execute(&self.pool)
        .await
        .expect("assessment");
    }

    async fn contact(&self, lead: &str, address: &str, confidence: f64) {
        sqlx::query(
            "INSERT INTO contact_emails (id, lead_id, address, provenance, method, confidence, kind, found_at)
             VALUES ($1, $2::uuid, $3, 'https://example.com/security.txt', 'security_txt', $4, 'role', now())",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(address)
        .bind(confidence)
        .execute(&self.pool)
        .await
        .expect("contact");
    }

    async fn signal(&self, lead: &str, kind: &str) {
        sqlx::query(
            "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint)
             VALUES ($1, $2::uuid, $3, 'keycloak', 'http_probe', '{}'::jsonb, 'https://example.com/', 1.0, now(), $4)",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(kind)
        .bind(format!("{lead}-{kind}"))
        .execute(&self.pool)
        .await
        .expect("signal");
    }

    async fn suppress(&self, kind: &str, value: &str) {
        sqlx::query(
            "INSERT INTO suppression_list (id, kind, value, created_at) VALUES ($1, $2, $3, now())",
        )
        .bind(common::generate_uuid_v7())
        .bind(kind)
        .bind(value)
        .execute(&self.pool)
        .await
        .expect("suppression");
    }
}

async fn get_raw(
    harness: &Harness,
    uri: &str,
    token: &str,
) -> (StatusCode, http::HeaderMap, String) {
    send_raw(
        harness.public(),
        Method::GET,
        uri,
        Some(&format!("Bearer {token}")),
        None,
    )
    .await
}

fn domains(body: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(body).expect("json");
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item["domain"].as_str().expect("domain").to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_json_export_lists_the_profile_leads_with_the_profile_id(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("Keycloak Ops").await;
    let other = seed.profile("other").await;
    let lead = seed.lead(&profile, "alpha.com", "validated", 9.0).await;
    seed.contact(&lead, "security@alpha.com", 0.9).await;
    seed.assessment(&lead, "Runs Keycloak").await;
    seed.signal(&lead, "product_detected").await;
    seed.lead(&other, "elsewhere.com", "validated", 9.0).await;
    let harness = Harness::new(pool).await;

    let (status, headers, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export"),
        &harness.valid_user_token(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "application/json");
    assert_eq!(
        headers[CONTENT_DISPOSITION],
        "attachment; filename=\"keycloak-ops-leads.json\""
    );
    let value: Value = serde_json::from_str(&body).unwrap();
    let items = value.as_array().unwrap();
    assert_eq!(items.len(), 1);
    let record = &items[0];
    assert_eq!(record["lead_id"], lead);
    assert_eq!(record["profile"]["id"], profile);
    assert_eq!(record["profile"]["name"], "Keycloak Ops");
    assert_eq!(record["domain"], "alpha.com");
    assert_eq!(record["status"], "validated");
    assert_eq!(record["contact"]["address"], "security@alpha.com");
    assert_eq!(record["assessment"]["summary"], "Runs Keycloak");
    assert_eq!(record["assessment"]["draft"]["status"], "approved");
    assert_eq!(
        record["assessment"]["evidence"][0]["quote"],
        "Powered by Keycloak"
    );
    assert_eq!(record["signals"][0]["kind"], "product_detected");
}

#[sqlx::test(migrations = "../../migrations")]
async fn filters_narrow_the_export_and_suppressed_leads_never_appear(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("alpha").await;
    seed.lead(&profile, "high.com", "validated", 9.0).await;
    seed.lead(&profile, "low.com", "validated", 1.0).await;
    seed.lead(&profile, "fresh.com", "new", 9.0).await;
    seed.lead(&profile, "gone.com", "validated", 9.0).await;
    seed.suppress("domain", "gone.com").await;
    let harness = Harness::new(pool).await;
    let token = harness.valid_user_token();

    let (_, _, all) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export"),
        &token,
    )
    .await;
    let mut all = domains(&all);
    all.sort();
    assert_eq!(all, vec!["fresh.com", "high.com", "low.com"]);

    let (status, _, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export?status=validated&min_score=5"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), vec!["high.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suppressed_address_is_not_exported_and_the_next_contact_is(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("alpha").await;
    let lead = seed.lead(&profile, "alpha.com", "validated", 9.0).await;
    seed.contact(&lead, "security@alpha.com", 0.95).await;
    seed.contact(&lead, "contact@alpha.com", 0.5).await;
    seed.suppress("email", "security@alpha.com").await;
    let harness = Harness::new(pool).await;

    let (_, _, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export"),
        &harness.valid_user_token(),
    )
    .await;

    assert!(!body.contains("security@alpha.com"));
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value[0]["contact"]["address"], "contact@alpha.com");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_csv_export_is_flat_attached_and_safe_against_formula_injection(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("Keycloak Ops").await;
    let lead = seed.lead(&profile, "alpha.com", "validated", 9.5).await;
    seed.contact(&lead, "security@alpha.com", 0.9).await;
    seed.assessment(&lead, "=HYPERLINK(\"http://evil\"), runs\nKeycloak")
        .await;
    seed.signal(&lead, "product_detected").await;
    seed.signal(&lead, "ct_certificate").await;
    let other = seed.profile("other").await;
    seed.lead(&other, "elsewhere.com", "validated", 9.0).await;
    let harness = Harness::new(pool).await;

    let (status, headers, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export?format=csv"),
        &harness.valid_user_token(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[CONTENT_TYPE], "text/csv; charset=utf-8");
    assert_eq!(
        headers[CONTENT_DISPOSITION],
        "attachment; filename=\"keycloak-ops-leads.csv\""
    );
    let (header, rows) = body.split_once("\r\n").expect("header");
    assert_eq!(
        header,
        "lead_id,profile_id,profile_name,domain,status,score,first_seen_at,last_seen_at,contact_address,contact_method,contact_confidence,summary,approach_angle,draft_subject,draft_status,signal_kinds,top_evidence_quote,top_evidence_provenance"
    );
    assert!(rows.starts_with(&format!(
        "{lead},{profile},Keycloak Ops,alpha.com,validated,9.5,"
    )));
    assert!(rows.contains(",security@alpha.com,security_txt,0.9,"));
    assert!(rows.contains("\"'=HYPERLINK(\"\"http://evil\"\"), runs\nKeycloak\""));
    assert!(rows.contains(",Hello,approved,"));
    assert!(rows.contains("product_detected") && rows.contains("ct_certificate"));
    assert!(rows.ends_with(",Powered by Keycloak,https://example.com/about\r\n"));
    assert!(!rows.contains("elsewhere.com"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn service_accounts_can_export(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("alpha").await;
    seed.lead(&profile, "alpha.com", "validated", 9.0).await;
    let harness = Harness::new(pool).await;

    let (status, _, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export"),
        &harness.valid_client_token(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), vec!["alpha.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_requests_are_rejected_without_data(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("alpha").await;
    let harness = Harness::new(pool).await;
    let token = harness.valid_user_token();

    let (status, _, _) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export?format=xml"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, _) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export?status=bogus"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _, _) =
        get_raw(&harness, "/api/v1/profiles/not-a-uuid/leads/export", &token).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let unknown = common::generate_uuid_v7();
    let (status, _, _) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{unknown}/leads/export"),
        &token,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_export_larger_than_the_cap_is_refused(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("alpha").await;
    sqlx::query(
        "INSERT INTO leads (id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at)
         SELECT gen_random_uuid(), $1::uuid, 'host' || n || '.com', 'new', 1, '[]'::jsonb, now(), now(), now()
         FROM generate_series(1, 10001) AS n",
    )
    .bind(&profile)
    .execute(&pool)
    .await
    .expect("bulk leads");
    let harness = Harness::new(pool).await;

    let (status, _, body) = get_raw(
        &harness,
        &format!("/api/v1/profiles/{profile}/leads/export"),
        &harness.valid_user_token(),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("narrow the filters"));
}

#[tokio::test]
async fn the_export_requires_authentication() {
    let harness = Harness::new(support::lazy_pool()).await;
    let id = common::generate_uuid_v7();
    let (status, _, _) = call(
        harness.public(),
        &format!("/api/v1/profiles/{id}/leads/export"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[test]
fn openapi_documents_the_export_with_bearer_auth() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).expect("spec");
    let operation = &spec["paths"]["/api/v1/profiles/{profile_id}/leads/export"]["get"];
    assert!(operation.is_object());
    assert!(
        operation["security"]
            .as_array()
            .is_some_and(|reqs| reqs.iter().any(|req| req.get("bearer_auth").is_some()))
    );
    assert!(operation["responses"]["200"].is_object());
}
