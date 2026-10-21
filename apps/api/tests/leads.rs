mod support;

use axum::Router;
use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::{PgPool, query};
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
            "INSERT INTO leads (id, profile_id, domain, status, score, first_seen_at, last_seen_at, updated_at)
             VALUES ($1, $2::uuid, $3, $4, $5, now() - make_interval(days => $6 + 10), now() - make_interval(days => $6), now())",
        )
        .bind(id)
        .bind(profile)
        .bind(domain)
        .bind(status)
        .bind(score)
        .bind(seen_day)
        .execute(&self.pool)
        .await
        .expect("lead");
        id.to_string()
    }

    async fn signal(&self, lead: &str, kind: &str) {
        sqlx::query(
            "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint, agent_id)
             VALUES ($1, $2::uuid, $3, 'keycloak', 'http_probe', '{\"status\": 200}'::jsonb, 'https://example.com/realms/master', 0.8, now(), $1::text, 'probe-a')",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(kind)
        .execute(&self.pool)
        .await
        .expect("signal");
    }

    async fn host_signal(&self, lead: &str, kind: &str, value: &str, source: &str, host: &str) {
        sqlx::query(
            "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint)
             VALUES ($1, $2::uuid, $3, $4, $5, $6, $7, 0.9, now(), $1::text)",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(kind)
        .bind(value)
        .bind(source)
        .bind(json!({"host": host}))
        .bind(format!("dns:{host}?type=CNAME"))
        .execute(&self.pool)
        .await
        .expect("host signal");
    }

    async fn candidate_host(&self, host: &str, domain: &str) {
        sqlx::query(
            "INSERT INTO candidate_hosts (host, domain, origin, first_seen_at, last_seen_at)
             VALUES ($1, $2, 'certificate_transparency', now(), now())",
        )
        .bind(host)
        .bind(domain)
        .execute(&self.pool)
        .await
        .expect("candidate host");
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
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at, draft_subject, draft_body, draft_status)
             VALUES ($1, $2::uuid, 2, 'Runs Keycloak', 'Offer managed IAM', $3, 'model-x', 'qualification-v1', now(), 'Hello', 'Body text', 'pending')",
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
        self.patch_body(lead, json!({ "status": status })).await
    }

    async fn patch_body(&self, lead: &str, body: Value) -> (StatusCode, Value) {
        let (status, body, _) = send(
            self.app(),
            Method::PATCH,
            &format!("/api/v1/leads/{lead}/status"),
            Some(&self.bearer),
            Some(body),
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
        ("verdict=maybe", "verdict"),
        ("verdict=fiable,nope", "verdict"),
        (
            "verdict=fiable%27%3B%20DROP%20TABLE%20leads%3B--",
            "verdict",
        ),
        ("min_confidence=101", "min_confidence"),
        ("min_confidence=-1", "min_confidence"),
        ("min_confidence=abc", "min_confidence"),
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

    assert!(data.get("score_breakdown").is_none());

    let signal = &data["signals"][0];
    assert_eq!(signal["kind"], "product_detected");
    assert_eq!(signal["source"], "http_probe");
    assert_eq!(signal["agent"], "probe-a");
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

const QUALIFIED_PROFILE: &str = r#"{
    "schema_version": 5,
    "identity": {"offer": "Managed IAM", "target_customer": "Companies running Keycloak"},
    "agents": [{"id": "ats-a", "name": "ATS", "source": {"kind": "ats", "keywords": ["sso"]}}],
    "qualification_agents": [
        {"id": "pertinence", "name": "Pertinence", "description": "La pertinence du lead.", "system_prompt": "p", "weight": 5,
         "questions": [{"id": "pertinence", "wording": "Pertinent ?", "weight": 5,
            "options": [
                {"value": "forte", "label": "Forte", "score": 1.0},
                {"value": "moyenne", "label": "Moyenne", "score": 0.5},
                {"value": "faible", "label": "Faible", "score": 0.0}]}]},
        {"id": "maturite", "name": "Maturité", "description": "La maturité technique.", "system_prompt": "m", "weight": 5,
         "questions": [{"id": "maturite", "wording": "Mature ?", "weight": 5,
            "options": [
                {"value": "oui", "label": "Oui", "score": 1.0},
                {"value": "non", "label": "Non", "score": 0.0}]}]}
    ],
    "aggregator": {"system_prompt": "a"},
    "scoring": {"factors": [{"signal": "product_detected", "weight": 10}]},
    "contact_ranking": {"preferred_local_parts": ["security"]},
    "llm": {"qualification_threshold": 50, "relevance_criteria": "c"}
}"#;

struct SeededResult<'a> {
    agent: &'a str,
    provider: Option<&'a str>,
    status: &'a str,
    summary: Option<&'a str>,
    answers: Value,
    score: Option<f64>,
    error: Option<&'a str>,
}

impl Seed {
    async fn qualified_profile(&self, name: &str) -> String {
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
            "INSERT INTO profile_revisions (profile_id, revision, definition, created_at) VALUES ($1, 1, $2, now()), ($1, 2, $2, now())",
        )
        .bind(id)
        .bind(serde_json::from_str::<Value>(QUALIFIED_PROFILE).expect("definition"))
        .execute(&self.pool)
        .await
        .expect("revisions");
        id.to_string()
    }

    async fn provider(&self, name: &str, model: &str) -> String {
        let id = common::generate_uuid_v7();
        sqlx::query(
            "INSERT INTO llm_providers (id, name, kind, base_url, model, timeout_ms, max_retries, input_cents_per_mtok, output_cents_per_mtok, enabled, created_at, updated_at)
             VALUES ($1, $2, 'openai_compatible', 'http://localhost:11434/v1', $3, 30000, 1, 0, 0, true, now(), now())",
        )
        .bind(id)
        .bind(name)
        .bind(model)
        .execute(&self.pool)
        .await
        .expect("provider");
        id.to_string()
    }

    async fn result(&self, lead: &str, seeded: SeededResult<'_>) {
        let SeededResult {
            agent,
            provider,
            status,
            summary,
            answers,
            score,
            error,
        } = seeded;
        sqlx::query(
            "INSERT INTO qualification_results (id, lead_id, profile_id, profile_revision, agent_id, provider_id, model, prompt_version, status, summary, answers, agent_score, error, created_at)
             SELECT $1, l.id, l.profile_id, 2, $3, $4::uuid, 'gemma_laya', 'hash', $5, $9, $6, $7, $8, now()
             FROM leads l WHERE l.id = $2::uuid",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(agent)
        .bind(provider)
        .bind(status)
        .bind(answers)
        .bind(score)
        .bind(error)
        .bind(summary)
        .execute(&self.pool)
        .await
        .expect("result");
        sqlx::query(
            "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at)
             VALUES ($1::uuid, 2, ARRAY['http_probe'], now()) ON CONFLICT (lead_id) DO NOTHING",
        )
        .bind(lead)
        .execute(&self.pool)
        .await
        .expect("state");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_carries_its_confidence_and_verdict(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let judged = seed.lead(&profile, "judged.com", "new", 10.0, 1).await;
    seed.lead(&profile, "pending.com", "new", 5.0, 2).await;
    sqlx::query("UPDATE leads SET confidence = 72.5, verdict = 'fiable', qualified_at = now() WHERE id = $1::uuid")
        .bind(&judged)
        .execute(&pool)
        .await
        .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;

    assert_eq!(status, StatusCode::OK);
    let leads = body["data"].as_array().unwrap();
    let find = |domain: &str| leads.iter().find(|lead| lead["domain"] == domain).unwrap();
    assert_eq!(find("judged.com")["confidence"], 72.5);
    assert_eq!(find("judged.com")["verdict"], "fiable");
    assert!(find("judged.com")["qualified_at"].is_string());
    assert!(find("pending.com")["confidence"].is_null());
    assert!(find("pending.com")["verdict"].is_null());
    assert!(find("pending.com")["qualified_at"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_lead_exposes_its_opinion_evidence_and_verdict_reason(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let scored = seed.lead(&profile, "scored.com", "new", 12.1, 1).await;
    seed.lead(&profile, "bare.com", "new", 5.0, 2).await;
    sqlx::query("UPDATE leads SET confidence = 51.4, verdict = 'a_verifier', qualified_at = now() WHERE id = $1::uuid")
        .bind(&scored)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_version, opinion, evidence, verdict_reason)
         VALUES ($1::uuid, 2, ARRAY['dns_cname'], now(), 1, 0.9286, $2, 'non_production')",
    )
    .bind(&scored)
    .bind(serde_json::json!({
        "score": 0.2564,
        "components": {"volume": 0.2835, "diversity": 0.5, "environment": 0.0, "strength": 0.242},
        "signal_count": 1,
        "signal_score": 12.1,
        "only_non_production": true,
        "independent_signals": 1,
        "decayed_signals": 0.5,
        "source_count": 1,
        "kind_count": 1
    }))
    .execute(&pool)
    .await
    .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;

    assert_eq!(status, StatusCode::OK);
    let leads = body["data"].as_array().unwrap();
    let find = |domain: &str| leads.iter().find(|lead| lead["domain"] == domain).unwrap();
    let lead = find("scored.com");
    assert_eq!(lead["opinion"], 92.9);
    assert_eq!(lead["evidence"]["strength"], 25.6);
    assert_eq!(lead["evidence"]["components"]["volume"], 28.4);
    assert_eq!(lead["evidence"]["components"]["diversity"], 50.0);
    assert_eq!(lead["evidence"]["components"]["environment"], 0.0);
    assert_eq!(lead["evidence"]["components"]["strength"], 24.2);
    assert_eq!(lead["evidence"]["signal_count"], 1);
    assert_eq!(lead["evidence"]["independent_signals"], 1);
    assert_eq!(lead["evidence"]["decayed_signals"], 0.5);
    assert_eq!(lead["evidence"]["source_count"], 1);
    assert_eq!(lead["evidence"]["kind_count"], 1);
    assert_eq!(lead["evidence"]["only_non_production"], true);
    assert_eq!(lead["verdict_reason"], "non_production");
    assert!(find("bare.com")["opinion"].is_null());
    assert!(find("bare.com")["evidence"].is_null());
    assert!(find("bare.com")["verdict_reason"].is_null());

    let (status, detail) = api.get(&format!("/api/v1/leads/{scored}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["data"]["lead"]["opinion"], 92.9);
    assert_eq!(detail["data"]["lead"]["verdict_reason"], "non_production");
    assert_eq!(detail["data"]["lead"]["evidence"]["strength"], 25.6);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_excluded_lead_exposes_its_exclusions_and_the_excluded_reason(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let excluded = seed.lead(&profile, "rival.com", "new", 12.1, 1).await;
    seed.lead(&profile, "plain.com", "new", 5.0, 2).await;
    sqlx::query("UPDATE leads SET confidence = 88.0, verdict = 'ecarte', qualified_at = now() WHERE id = $1::uuid")
        .bind(&excluded)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_version, opinion, evidence, verdict_reason, exclusions)
         VALUES ($1::uuid, 2, ARRAY['dns_cname'], now(), 2, 0.9, $2, 'excluded', $3)",
    )
    .bind(&excluded)
    .bind(serde_json::json!({
        "score": 0.8,
        "components": {"volume": 0.8, "diversity": 1.0, "environment": 1.0, "strength": 0.4},
        "signal_count": 3,
        "signal_score": 12.1,
        "only_non_production": false
    }))
    .bind(serde_json::json!([{
        "agent_id": "exclusions",
        "agent_name": "Exclusions",
        "question_id": "concurrent",
        "option": "oui",
        "label": "Concurrent",
        "probability": 0.85,
        "evidence": "Nous sommes un concurrent direct.",
        "evidence_source": "https://rival.com/about"
    }]))
    .execute(&pool)
    .await
    .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;

    assert_eq!(status, StatusCode::OK);
    let leads = body["data"].as_array().unwrap();
    let find = |domain: &str| leads.iter().find(|lead| lead["domain"] == domain).unwrap();
    let lead = find("rival.com");
    assert_eq!(lead["verdict"], "ecarte");
    assert_eq!(lead["verdict_reason"], "excluded");
    assert_eq!(lead["confidence"], 88.0);
    assert_eq!(
        lead["exclusions"],
        serde_json::json!([{
            "agent_id": "exclusions",
            "agent_name": "Exclusions",
            "question_id": "concurrent",
            "option": "oui",
            "label": "Concurrent",
            "probability": 85.0,
            "evidence": "Nous sommes un concurrent direct.",
            "evidence_source": "https://rival.com/about"
        }])
    );
    assert_eq!(find("plain.com")["exclusions"], serde_json::json!([]));

    let (status, detail) = api.get(&format!("/api/v1/leads/{excluded}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["data"]["lead"]["verdict_reason"], "excluded");
    assert_eq!(
        detail["data"]["lead"]["exclusions"][0]["label"],
        "Concurrent"
    );
}

async fn judged_fixture(pool: &PgPool) -> (Seed, String) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    for (domain, score, judgement) in [
        ("sure.com", 5.0, Some((92.5, "fiable"))),
        ("maybe.com", 50.0, Some((55.0, "a_verifier"))),
        ("out.com", 90.0, Some((8.0, "ecarte"))),
        ("waiting.com", 70.0, None),
    ] {
        let id = seed.lead(&profile, domain, "new", score, 1).await;
        if let Some((confidence, verdict)) = judgement {
            sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE id = $1::uuid")
                .bind(&id)
                .bind(confidence)
                .bind(verdict)
                .execute(pool)
                .await
                .unwrap();
        }
    }
    (seed, profile)
}

#[sqlx::test(migrations = "../../migrations")]
async fn leads_are_sorted_by_confidence_by_default_pending_ones_last(pool: PgPool) {
    let (_seed, profile) = judged_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, default) = api.get(&base).await;
    assert_eq!(
        domains(&default),
        ["sure.com", "maybe.com", "out.com", "waiting.com"]
    );

    let (_, ascending) = api.get(&format!("{base}?sort=confidence_asc")).await;
    assert_eq!(
        domains(&ascending),
        ["out.com", "maybe.com", "sure.com", "waiting.com"]
    );

    let (_, by_score) = api.get(&format!("{base}?sort=score_desc")).await;
    assert_eq!(
        domains(&by_score),
        ["out.com", "waiting.com", "maybe.com", "sure.com"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_verdict_and_confidence_filters_narrow_the_list_and_the_total(pool: PgPool) {
    let (_seed, profile) = judged_fixture(&pool).await;
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, visible) = api
        .get(&format!("{base}?verdict=fiable,a_verifier,en_attente"))
        .await;
    assert_eq!(domains(&visible), ["sure.com", "maybe.com", "waiting.com"]);
    assert_eq!(visible["pagination"]["total"], 3);

    let (_, discarded) = api.get(&format!("{base}?verdict=ecarte")).await;
    assert_eq!(domains(&discarded), ["out.com"]);

    let (_, pending) = api.get(&format!("{base}?verdict=en_attente")).await;
    assert_eq!(domains(&pending), ["waiting.com"]);

    let (_, confident) = api.get(&format!("{base}?min_confidence=55")).await;
    assert_eq!(domains(&confident), ["sure.com", "maybe.com"]);

    let (_, combined) = api
        .get(&format!(
            "{base}?verdict=a_verifier,ecarte&min_confidence=10"
        ))
        .await;
    assert_eq!(domains(&combined), ["maybe.com"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_lead_of_the_list_carries_the_latest_summary(pool: PgPool) {
    let (_seed, profile) = judged_fixture(&pool).await;
    let sure: uuid::Uuid = sqlx::query_scalar("SELECT id FROM leads WHERE domain = 'sure.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (age, summary) in [
        (20.0_f64, "older"),
        (5.0, "Le domaine expose un Keycloak ancien."),
    ] {
        sqlx::query(
            "INSERT INTO assessments (id, lead_id, profile_revision, summary, approach_angle, evidence, model, prompt_version, created_at)
             VALUES ($1, $2, 1, $3, 'angle', '[]'::jsonb, 'm', 'aggregator@x', now() - make_interval(secs => $4))",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(sure)
        .bind(summary)
        .bind(age)
        .execute(&pool)
        .await
        .unwrap();
    }
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/profiles/{profile}/leads")).await;

    let leads = body["data"].as_array().unwrap();
    let find = |domain: &str| leads.iter().find(|lead| lead["domain"] == domain).unwrap();
    assert_eq!(
        find("sure.com")["summary"],
        "Le domaine expose un Keycloak ancien."
    );
    assert!(find("maybe.com")["summary"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_assessment_exposes_the_narrative_and_the_evidence_flag(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "qualified", 5.0, 1).await;
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, verdict_narrative, approach_angle, evidence, evidence_verified, model, prompt_version, created_at)
         VALUES ($1, $2::uuid, 1, 's', 'Les agents concordent.', 'angle', '[]'::jsonb, false, 'm', 'aggregator@x', now())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    let assessment = &body["data"]["assessments"][0];
    assert_eq!(assessment["verdict_narrative"], "Les agents concordent.");
    assert_eq!(assessment["evidence_verified"], false);
    assert_eq!(assessment["quality_warnings"], json!([]));
    assert_eq!(body["data"]["lead"]["summary"], "s");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_assessment_exposes_its_quality_warnings(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "qualified", 5.0, 1).await;
    sqlx::query(
        "INSERT INTO assessments (id, lead_id, profile_revision, summary, verdict_narrative, approach_angle, evidence, evidence_verified, quality_warnings, model, prompt_version, created_at)
         VALUES ($1, $2::uuid, 1, 's', 'n', 'angle', '[]'::jsonb, true, ARRAY['repetition', 'restated_figures'], 'm', 'aggregator@x', now())",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(
        body["data"]["assessments"][0]["quality_warnings"],
        json!(["repetition", "restated_figures"])
    );
}

async fn seed_state(pool: &PgPool, lead: &str, attempts: i32, error: Option<&str>) {
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, summary_attempts, summary_error, summary_next_attempt_at)
         VALUES ($1::uuid, 2, ARRAY['http_probe'], now(), $2, $3, now() + interval '6 hours')",
    )
    .bind(lead)
    .bind(attempts)
    .bind(error)
    .execute(pool)
    .await
    .expect("state");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_reports_a_failing_summary_and_retry_resets_it(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 5.0, 1).await;
    seed_state(&pool, &lead, 5, Some("échec de l'appel au modèle")).await;
    let api = Api::new(pool.clone()).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;
    let state = &body["data"]["summary_state"];
    assert_eq!(state["attempts"], 5);
    assert_eq!(state["failed"], true);
    assert_eq!(state["error"], "échec de l'appel au modèle");
    assert!(state["next_attempt_at"].is_string());

    let uri = format!("/api/v1/leads/{lead}/summary/retry");
    let (status, _, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(status, StatusCode::OK);

    let (_, after) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert!(after["data"]["summary_state"].is_null());
    let (attempts, next): (i32, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT summary_attempts, summary_next_attempt_at FROM lead_qualification_state",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempts, 0);
    assert!(next.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_few_failures_are_reported_but_not_as_failed(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 5.0, 1).await;
    seed_state(&pool, &lead, 2, Some("réponse non conforme au contrat")).await;
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(body["data"]["summary_state"]["attempts"], 2);
    assert_eq!(body["data"]["summary_state"]["failed"], false);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_retry_is_for_users_and_for_qualified_visible_leads_only(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 5.0, 1).await;
    let unqualified = seed.lead(&profile, "bare.com", "new", 5.0, 1).await;
    seed_state(&pool, &lead, 5, Some("x")).await;
    let api = Api::new(pool).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());

    let uri = format!("/api/v1/leads/{lead}/summary/retry");
    let (forbidden, _, _) = send(api.app(), Method::POST, &uri, Some(&client), None).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = send(api.app(), Method::POST, &uri, None, None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

    let uri = format!("/api/v1/leads/{unqualified}/summary/retry");
    let (conflict, _, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(conflict, StatusCode::CONFLICT);

    let uri = format!("/api/v1/leads/{}/summary/retry", common::generate_uuid_v7());
    let (missing, _, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
    let (malformed, _, _) = send(
        api.app(),
        Method::POST,
        "/api/v1/leads/nope/summary/retry",
        Some(&api.bearer),
        None,
    )
    .await;
    assert_eq!(malformed, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn each_verdict_is_served_with_its_wire_name(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    for (domain, verdict) in [
        ("a.com", "fiable"),
        ("b.com", "a_verifier"),
        ("c.com", "ecarte"),
    ] {
        let id = seed.lead(&profile, domain, "new", 1.0, 1).await;
        sqlx::query("UPDATE leads SET confidence = 50, verdict = $2 WHERE id = $1::uuid")
            .bind(&id)
            .bind(verdict)
            .execute(&pool)
            .await
            .unwrap();
        let api = Api::new(pool.clone()).await;
        let (_, body) = api.get(&format!("/api/v1/leads/{id}")).await;
        assert_eq!(body["data"]["lead"]["verdict"], verdict);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_lists_the_qualification_results_with_labels_and_providers(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "detail.com", "new", 8.0, 1).await;
    let provider = seed.provider("Laya", "gemma_laya").await;
    seed.result(
        &lead,
        SeededResult {
            agent: "maturite",
            provider: Some(&provider),
            status: "invalid",
            summary: None,
            answers: json!({}),
            score: None,
            error: Some("Réponse non conforme au contrat : question `maturite` : pas de réponse"),
        },
    )
    .await;
    seed.result(
        &lead,
        SeededResult {
            agent: "pertinence",
            provider: Some(&provider),
            status: "valid",
            summary: Some("Le lead exploite un Keycloak ancien."),
            answers: json!({"pertinence": {"type": "choice", "choice": "moyenne", "confidence": 0.8,
                "evidence": "expose Keycloak 18", "evidence_source": "https://detail.com/realms/master",
                "grounded": true,
                "probabilities": {"forte": 0.25, "moyenne": 0.5, "faible": 0.25, "inconnu": 0.0}}}),
            score: Some(0.5),
            error: None,
        },
    )
    .await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let results = body["data"]["qualification_results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    let first = &results[0];
    assert_eq!(first["agent_id"], "pertinence");
    assert_eq!(first["agent_name"], "Pertinence");
    assert_eq!(first["agent_description"], "La pertinence du lead.");
    assert_eq!(first["summary"], "Le lead exploite un Keycloak ancien.");
    assert_eq!(first["provider_name"], "Laya");
    assert_eq!(first["provider_id"], provider);
    assert_eq!(first["model"], "gemma_laya");
    assert_eq!(first["provenance"]["model"], "gemma_laya");
    assert_eq!(first["provenance"]["samples"], 1);
    assert!(first["provenance"]["generation"].is_object());
    assert_eq!(first["status"], "valid");
    assert_eq!(first["agent_score"], 0.5);
    assert!(first["error"].is_null());
    let answer = &first["answers"][0];
    assert_eq!(answer["question_id"], "pertinence");
    assert_eq!(answer["wording"], "Pertinent ?");
    assert_eq!(answer["choice"], "moyenne");
    assert_eq!(answer["choice_label"], "Moyenne");
    assert_eq!(answer["confidence"], 0.8);
    assert_eq!(answer["evidence"], "expose Keycloak 18");
    assert_eq!(
        answer["evidence_source"],
        "https://detail.com/realms/master"
    );
    assert_eq!(answer["grounded"], true);
    assert!(answer["original_choice"].is_null());
    assert_eq!(answer["probabilities"][0]["value"], "forte");
    assert_eq!(answer["probabilities"][0]["label"], "Forte");
    assert_eq!(answer["probabilities"][0]["probability"], 0.25);
    assert_eq!(answer["probabilities"][1]["probability"], 0.5);
    let invalid = &results[1];
    assert_eq!(invalid["agent_id"], "maturite");
    assert_eq!(invalid["agent_name"], "Maturité");
    assert_eq!(invalid["agent_description"], "La maturité technique.");
    assert!(invalid["summary"].is_null());
    assert_eq!(invalid["status"], "invalid");
    assert!(invalid["agent_score"].is_null());
    assert_eq!(invalid["answers"].as_array().unwrap().len(), 0);
    assert!(invalid["error"].as_str().unwrap().contains("maturite"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_lists_the_scoring_results_and_the_combined_decision(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "scored.com", "new", 8.0, 1).await;
    let other = seed.lead(&profile, "pending.com", "new", 7.0, 2).await;
    let provider = seed.provider("Synthèse", "gemma_laya").await;
    seed.result(
        &lead,
        SeededResult {
            agent: "pertinence",
            provider: None,
            status: "valid",
            summary: Some("Résumé."),
            answers: json!({}),
            score: Some(0.5),
            error: None,
        },
    )
    .await;
    sqlx::query(
        "INSERT INTO scoring_results (id, lead_id, profile_id, profile_revision, agent_id, provider_id, model, prompt_version, status, criteria, score, verdict, reasons, created_at, provider_name, samples)
         SELECT $1, l.id, l.profile_id, 2, 'score-global', $3::uuid, 'gemma_laya', 'scoring-agent@hash', 'valid', $4, 72, $5, $6, now(), 'Synthèse', 3
         FROM leads l WHERE l.id = $2::uuid",
    )
    .bind(common::generate_uuid_v7())
    .bind(&lead)
    .bind(&provider)
    .bind(json!({"adequation_offre": {"type": "choice", "choice": "forte", "confidence": 0.8,
        "evidence": "expose Keycloak 18", "evidence_source": "https://scored.com/realms/master",
        "grounded": true, "agreement": 0.67,
        "probabilities": {"forte": 0.8, "moyenne": 0.1, "faible": 0.0, "inconnu": 0.1}}}))
    .bind(json!({"choice": "a_verifier", "declared_choice": "fiable", "confidence": 0.6,
        "probabilities": {"fiable": 0.3, "a_verifier": 0.6, "ecarte": 0.1}}))
    .bind(json!(["Un seul signal.", "Entreprise ciblée."]))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE leads SET confidence = 60.0, verdict = 'a_verifier', llm_score = 72, qualified_at = now() WHERE id = $1::uuid")
        .bind(&lead)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE lead_qualification_state SET verdict_source = 'scoring', indicator_confidence = 41.5, indicator_cap = 'single_angle', verdict_probabilities = $2, evidence = $3, opinion = 0.7 WHERE lead_id = $1::uuid",
    )
    .bind(&lead)
    .bind(json!({"fiable": 0.3, "a_verifier": 0.6, "ecarte": 0.1}))
    .bind(json!({
        "score": 0.4, "components": {"volume": 0.4, "diversity": 0.5, "environment": 1.0, "strength": 0.2},
        "signal_count": 1, "signal_score": 8.0, "only_non_production": false,
        "independent_signals": 1, "decayed_signals": 1.0, "source_count": 1, "kind_count": 1
    }))
    .execute(&pool)
    .await
    .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["lead"]["llm_score"], 72);
    assert_eq!(data["lead"]["verdict"], "a_verifier");
    assert_eq!(data["lead"]["confidence"], 60.0);
    assert_eq!(data["lead"]["verdict_source"], "scoring");
    assert_eq!(data["lead"]["indicator_confidence"], 41.5);
    assert_eq!(data["lead"]["indicator_cap"], "single_angle");
    assert_eq!(data["lead"]["verdict_probabilities"]["a_verifier"], 60.0);
    assert_eq!(data["lead"]["verdict_probabilities"]["fiable"], 30.0);
    let results = data["scoring_results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result["agent_id"], "score-global");
    assert_eq!(result["agent_name"], "Score global");
    assert_eq!(result["weight"], 5);
    assert_eq!(result["provider_name"], "Synthèse");
    assert_eq!(result["model"], "gemma_laya");
    assert_eq!(result["provenance"]["samples"], 3);
    assert_eq!(result["status"], "valid");
    assert_eq!(result["score"], 72);
    assert_eq!(result["verdict"]["choice"], "a_verifier");
    assert_eq!(result["verdict"]["declared_choice"], "fiable");
    assert_eq!(result["verdict"]["confidence"], 60.0);
    assert_eq!(result["verdict"]["probabilities"]["ecarte"], 10.0);
    assert_eq!(
        result["reasons"],
        json!(["Un seul signal.", "Entreprise ciblée."])
    );
    let criterion = &result["criteria"][0];
    assert_eq!(criterion["question_id"], "adequation_offre");
    assert_eq!(criterion["choice"], "forte");
    assert_eq!(criterion["choice_label"], "Forte");
    assert_eq!(criterion["evidence"], "expose Keycloak 18");
    assert_eq!(criterion["grounded"], true);
    assert_eq!(criterion["agreement"], 0.67);

    let (_, pending) = api.get(&format!("/api/v1/leads/{other}")).await;
    assert!(
        pending["data"]["scoring_results"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(pending["data"]["lead"]["llm_score"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_shows_the_raw_and_calibrated_probabilities_of_a_calibrated_agent(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let calibrated = seed.lead(&profile, "calibrated.com", "new", 8.0, 1).await;
    let plain = seed.lead(&profile, "plain.com", "new", 7.0, 2).await;
    for lead in [&calibrated, &plain] {
        seed.result(
            lead,
            SeededResult {
                agent: "pertinence",
                provider: None,
                status: "valid",
                summary: None,
                answers: json!({}),
                score: Some(0.5),
                error: None,
            },
        )
        .await;
        sqlx::query(
            "INSERT INTO scoring_results (id, lead_id, profile_id, profile_revision, agent_id, model, prompt_version, status, criteria, score, verdict, reasons, created_at, provider_name, samples)
             SELECT $1, l.id, l.profile_id, 2, 'score-global', 'gemma_laya', 'scoring-agent@hash', 'valid', '{}'::jsonb, 72, $3, '[\"r\"]'::jsonb, now(), 'Synthèse', 1
             FROM leads l WHERE l.id = $2::uuid",
        )
        .bind(common::generate_uuid_v7())
        .bind(lead)
        .bind(json!({"choice": "fiable", "confidence": 0.7,
            "probabilities": {"fiable": 0.7, "a_verifier": 0.2, "ecarte": 0.1}}))
        .execute(&pool)
        .await
        .unwrap();
    }
    let api = Api::new(pool.clone()).await;

    let (_, before) = api.get(&format!("/api/v1/leads/{calibrated}")).await;
    assert!(before["data"]["scoring_results"][0]["calibration"].is_null());

    sqlx::query(
        "INSERT INTO calibrations (profile_id, kind, agent_id, points, n, positives, brier_before, brier_after, fitted_at, enabled)
         VALUES ($1::uuid, 'scoring', 'score-global', $2, 80, 30, 0.3, 0.2, now(), true)",
    )
    .bind(&profile)
    .bind(json!([{"x": 0.0, "y": 0.4}]))
    .execute(&pool)
    .await
    .unwrap();

    let (status, body) = api.get(&format!("/api/v1/leads/{calibrated}")).await;

    assert_eq!(status, StatusCode::OK);
    let result = &body["data"]["scoring_results"][0];
    assert_eq!(result["verdict"]["probabilities"]["fiable"], 70.0);
    assert_eq!(result["verdict"]["probabilities"]["a_verifier"], 20.0);
    let calibration = &result["calibration"];
    assert_eq!(calibration["n"], 80);
    assert_eq!(calibration["probabilities"]["fiable"], 30.0);
    assert_eq!(calibration["probabilities"]["a_verifier"], 20.0);
    assert_eq!(calibration["probabilities"]["ecarte"], 50.0);

    sqlx::query("UPDATE calibrations SET enabled = false")
        .execute(&pool)
        .await
        .unwrap();
    let (_, disabled) = api.get(&format!("/api/v1/leads/{plain}")).await;
    assert!(disabled["data"]["scoring_results"][0]["calibration"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unscored_lead_waits_for_its_scoring_and_the_list_sorts_by_llm_score(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let low = seed.lead(&profile, "low.com", "new", 30.0, 1).await;
    let high = seed.lead(&profile, "high.com", "new", 10.0, 2).await;
    seed.lead(&profile, "waiting.com", "new", 50.0, 3).await;
    for (lead, score) in [(&low, 20), (&high, 90)] {
        sqlx::query("UPDATE leads SET llm_score = $2, confidence = 50, verdict = 'a_verifier' WHERE id = $1::uuid")
            .bind(lead)
            .bind(score)
            .execute(&pool)
            .await
            .unwrap();
    }
    let api = Api::new(pool).await;
    let base = format!("/api/v1/profiles/{profile}/leads");

    let (_, desc) = api.get(&format!("{base}?sort=llm_score_desc")).await;
    assert_eq!(domains(&desc), ["high.com", "low.com", "waiting.com"]);
    let (_, asc) = api.get(&format!("{base}?sort=llm_score_asc")).await;
    assert_eq!(domains(&asc), ["low.com", "high.com", "waiting.com"]);
    let (_, pending) = api.get(&format!("{base}?verdict=en_attente")).await;
    assert_eq!(domains(&pending), ["waiting.com"]);
    let waiting = &pending["data"][0];
    assert!(waiting["llm_score"].is_null());
    assert!(waiting["verdict"].is_null());
    let (_, listed) = api.get(&base).await;
    let scores: Vec<(&str, Option<u64>)> = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|lead| (lead["domain"].as_str().unwrap(), lead["llm_score"].as_u64()))
        .collect();
    assert!(scores.contains(&("high.com", Some(90))));
    assert!(scores.contains(&("waiting.com", None)));
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_flags_a_downgraded_answer_and_keeps_old_answers_readable(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "downgraded.com", "new", 8.0, 1).await;
    seed.result(
        &lead,
        SeededResult {
            agent: "pertinence",
            provider: None,
            status: "valid",
            summary: Some("Résumé."),
            answers: json!({"pertinence": {"type": "choice", "choice": "inconnu", "confidence": 0.8,
                "evidence": "phrase inventée", "grounded": false, "original_choice": "forte",
                "probabilities": {"forte": 0.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 1.0}}}),
            score: Some(0.5),
            error: None,
        },
    )
    .await;
    seed.result(
        &lead,
        SeededResult {
            agent: "maturite",
            provider: None,
            status: "valid",
            summary: Some("Résumé."),
            answers: json!({"maturite": {"type": "choice", "choice": "oui", "confidence": 0.8,
                "probabilities": {"oui": 1.0, "non": 0.0, "inconnu": 0.0}}}),
            score: Some(1.0),
            error: None,
        },
    )
    .await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let results = body["data"]["qualification_results"].as_array().unwrap();
    let downgraded = &results[0]["answers"][0];
    assert_eq!(downgraded["choice"], "inconnu");
    assert_eq!(downgraded["choice_label"], "Inconnu");
    assert_eq!(downgraded["grounded"], false);
    assert_eq!(downgraded["original_choice"], "forte");
    assert_eq!(downgraded["original_choice_label"], "Forte");
    let old = &results[1]["answers"][0];
    assert_eq!(old["grounded"], true);
    assert_eq!(old["evidence"], "");
    assert!(old["evidence_source"].is_null());
    assert!(old["original_choice"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn detail_exposes_the_agreement_per_axis_and_the_escalation_mark(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "voted.com", "new", 8.0, 1).await;
    seed.result(
        &lead,
        SeededResult {
            agent: "pertinence",
            provider: None,
            status: "valid",
            summary: Some("Résumé."),
            answers: json!({"pertinence": {"type": "choice", "choice": "forte", "confidence": 0.8,
                "evidence": "expose Keycloak 18", "grounded": true, "agreement": 0.6666666666666666,
                "probabilities": {"forte": 0.7, "moyenne": 0.2, "faible": 0.1, "inconnu": 0.0}}}),
            score: Some(0.8),
            error: None,
        },
    )
    .await;
    seed.result(
        &lead,
        SeededResult {
            agent: "maturite",
            provider: None,
            status: "valid",
            summary: Some("Résumé."),
            answers: json!({"maturite": {"type": "choice", "choice": "oui", "confidence": 0.8,
                "probabilities": {"oui": 1.0, "non": 0.0, "inconnu": 0.0}}}),
            score: Some(1.0),
            error: None,
        },
    )
    .await;
    sqlx::query("UPDATE qualification_results SET samples = 3 WHERE agent_id = 'pertinence'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE qualification_results SET escalated = true WHERE agent_id = 'maturite'")
        .execute(&pool)
        .await
        .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let results = body["data"]["qualification_results"].as_array().unwrap();
    assert_eq!(results[0]["escalated"], false);
    assert_eq!(results[0]["provenance"]["samples"], 3);
    assert_eq!(results[0]["answers"][0]["agreement"], 0.6666666666666666);
    assert_eq!(results[1]["escalated"], true);
    assert!(results[1]["answers"][0]["agreement"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_exposes_the_facts_and_the_signal_sentences(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "luzernrisk.com", "new", 1.0, 1).await;
    seed.host_signal(
        &lead,
        "saas_provider",
        "auth0",
        "dns_cname",
        "login.dev.luzernrisk.com",
    )
    .await;
    seed.candidate_host("login.dev.luzernrisk.com", "luzernrisk.com")
        .await;
    seed.candidate_host("www.luzernrisk.com", "luzernrisk.com")
        .await;
    seed.candidate_host("www.other.com", "other.com").await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(
        data["signals"][0]["sentence"],
        "login.dev.luzernrisk.com pointe vers Auth0 (CNAME)"
    );
    let facts = &data["facts"];
    assert_eq!(facts["hosts"][0]["host"], "login.dev.luzernrisk.com");
    assert_eq!(facts["hosts"][0]["environment"], "development");
    assert_eq!(facts["hosts"][0]["signals"], 1);
    assert_eq!(facts["distinct_hosts"], 1);
    assert_eq!(facts["production_hosts"], 0);
    assert_eq!(facts["production_share"], 0.0);
    assert_eq!(facts["only_non_production"], true);
    assert_eq!(facts["distinct_subdomains"], 2);
    assert_eq!(facts["signal_count"], 1);
    assert_eq!(facts["source_count"], 1);
    assert_eq!(facts["sources"], json!(["dns_cname"]));
    assert!(facts["first_seen_at"].is_string());
    assert!(facts["last_seen_at"].is_string());
    assert!(facts["probable_country"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_facts_name_the_probable_country_and_stay_empty_without_hosts(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "bare.fr", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let facts = &body["data"]["facts"];
    assert_eq!(facts["hosts"], json!([]));
    assert!(facts["production_share"].is_null());
    assert_eq!(facts["only_non_production"], false);
    assert_eq!(facts["signal_count"], 0);
    assert_eq!(facts["probable_country"]["code"], "FR");
}

const OFFICIAL: &str = r#"{
    "siren": "552081317",
    "legal_name": "ELECTRICITE DE FRANCE",
    "naf_code": "35.11Z",
    "naf_label": "Production et distribution d'électricité",
    "headcount_code": "53",
    "headcount_range": "10 000 salariés et plus",
    "head_office_city": "PARIS",
    "head_office_postcode": "75008",
    "head_office_country": "France",
    "head_office_country_code": "FR",
    "created_on": "1955-01-01"
}"#;

async fn seed_dossier(pool: &PgPool, domain: &str, state: &str, with_official: bool) {
    let official: Option<Value> = with_official.then(|| serde_json::from_str(OFFICIAL).unwrap());
    sqlx::query(
        "INSERT INTO company_dossiers (domain, state, siren, official, pages, built_at, next_attempt_at)
         VALUES ($1, $2, $3, $4, $5, now(), now() + interval '90 days')",
    )
    .bind(domain)
    .bind(state)
    .bind(with_official.then_some("552081317"))
    .bind(official)
    .bind(json!([
        {"url": "https://acme.com/", "text": "Bienvenue"},
        {"url": "https://acme.com/mentions-legales", "text": "SIREN 552 081 317"}
    ]))
    .execute(pool)
    .await
    .expect("dossier");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_carries_the_company_dossier_and_the_official_facts(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "acme.com", "new", 1.0, 1).await;
    seed_dossier(&pool, "acme.com", "complete", true).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    let company = &body["data"]["company"];
    assert_eq!(company["state"], "complete");
    assert_eq!(company["siren"], "552081317");
    assert_eq!(company["official"]["legal_name"], "ELECTRICITE DE FRANCE");
    assert_eq!(company["official"]["naf_code"], "35.11Z");
    assert_eq!(
        company["official"]["headcount_range"],
        "10 000 salariés et plus"
    );
    assert_eq!(company["official"]["head_office_city"], "PARIS");
    assert_eq!(company["official"]["created_on"], "1955-01-01");
    assert_eq!(
        company["official"]["annuaire_url"],
        "https://annuaire-entreprises.data.gouv.fr/entreprise/552081317"
    );
    assert_eq!(
        company["pages"],
        json!([
            {"url": "https://acme.com/"},
            {"url": "https://acme.com/mentions-legales"}
        ])
    );
    assert!(company["built_at"].is_string());
    let facts = &body["data"]["facts"];
    assert_eq!(facts["official"]["legal_name"], "ELECTRICITE DE FRANCE");
    assert_eq!(facts["official"]["siren"], "552081317");
    assert_eq!(facts["probable_country"]["code"], "FR");
    assert_eq!(facts["country_basis"], "head_office");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_without_a_dossier_has_no_company_and_keeps_the_extension_basis(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "bare.fr", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert!(body["data"]["company"].is_null());
    assert!(body["data"]["facts"]["official"].is_null());
    assert_eq!(body["data"]["facts"]["country_basis"], "tld");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_company_refresh_is_for_users_and_makes_the_dossier_due_now(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "acme.com", "new", 1.0, 1).await;
    let api = Api::new(pool.clone()).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let uri = format!("/api/v1/leads/{lead}/company/refresh");

    let (forbidden, _, _) = send(api.app(), Method::POST, &uri, Some(&client), None).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = send(api.app(), Method::POST, &uri, None, None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

    let (created, body, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(created, StatusCode::ACCEPTED);
    assert_eq!(body["data"]["state"], "pending");
    let due: bool = sqlx::query_scalar(
        "SELECT next_attempt_at <= now() FROM company_dossiers WHERE domain = 'acme.com'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(due);

    sqlx::query("UPDATE company_dossiers SET state = 'complete', next_attempt_at = now() + interval '90 days'")
        .execute(&pool)
        .await
        .unwrap();
    let (again, body, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(again, StatusCode::ACCEPTED);
    assert_eq!(body["data"]["state"], "complete");
    let due: bool = sqlx::query_scalar("SELECT next_attempt_at <= now() FROM company_dossiers")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(due);

    let unknown = format!(
        "/api/v1/leads/{}/company/refresh",
        common::generate_uuid_v7()
    );
    let (missing, _, _) = send(api.app(), Method::POST, &unknown, Some(&api.bearer), None).await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
    let (malformed, _, _) = send(
        api.app(),
        Method::POST,
        "/api/v1/leads/nope/company/refresh",
        Some(&api.bearer),
        None,
    )
    .await;
    assert_eq!(malformed, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_without_results_has_an_empty_qualification_list(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "bare.com", "new", 1.0, 1).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{lead}")).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["qualification_results"], json!([]));
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
async fn rejecting_a_lead_requires_a_known_reason_and_a_bounded_note(pool: PgPool) {
    let (seed, profile) = listing_fixture(&pool).await;
    let lead = seed.lead(&profile, "nope.com", "qualified", 1.0, 1).await;
    let api = Api::new(pool.clone()).await;

    for (body, path) in [
        (json!({"status": "rejected"}), "reason"),
        (
            json!({"status": "rejected", "reason": "pas_une_raison"}),
            "reason",
        ),
        (json!({"status": "rejected", "reason": "autre"}), "note"),
        (
            json!({"status": "rejected", "reason": "concurrent", "note": "n".repeat(501)}),
            "note",
        ),
    ] {
        let (status, response) = api.patch_body(&lead, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(response["details"][0]["path"], path, "{body}");
    }
    let (_, detail) = api.get(&format!("/api/v1/leads/{lead}")).await;
    assert_eq!(detail["data"]["lead"]["status"], "qualified");
    let labels: i64 = sqlx::query_scalar("SELECT count(*) FROM decision_labels")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(labels, 0);
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

async fn link_company(pool: &PgPool, domain: &str, siren: &str, legal_name: &str) {
    sqlx::query(
        "INSERT INTO companies (siren, legal_name) VALUES ($1, $2) ON CONFLICT (siren) DO NOTHING",
    )
    .bind(siren)
    .bind(legal_name)
    .execute(pool)
    .await
    .expect("company");
    sqlx::query(
        "INSERT INTO company_dossiers (domain, state, siren, official, pages, built_at, next_attempt_at, company_siren)
         VALUES ($1, 'complete', $2, $3, '[]'::jsonb, now(), now() + interval '90 days', $2)",
    )
    .bind(domain)
    .bind(siren)
    .bind(json!({"siren": siren, "legal_name": legal_name}))
    .execute(pool)
    .await
    .expect("dossier");
}

async fn judge(pool: &PgPool, lead: &str, confidence: f64, verdict: &str) {
    sqlx::query("UPDATE leads SET confidence = $2, verdict = $3 WHERE id = $1::uuid")
        .bind(lead)
        .bind(confidence)
        .bind(verdict)
        .execute(pool)
        .await
        .expect("judgement");
}

struct Group {
    seed: Seed,
    profile: String,
    other_profile: String,
    first: String,
    second: String,
    elsewhere: String,
}

async fn group_fixture(pool: &PgPool) -> Group {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let other_profile = seed.profile("auth-ops").await;
    let first = seed.lead(&profile, "acme.com", "new", 10.0, 3).await;
    let second = seed.lead(&profile, "acme.fr", "new", 20.0, 2).await;
    let elsewhere = seed.lead(&other_profile, "acme.com", "new", 5.0, 1).await;
    seed.lead(&profile, "solo.io", "new", 30.0, 1).await;
    link_company(pool, "acme.com", "552081317", "ACME SAS").await;
    link_company(pool, "acme.fr", "552081317", "ACME SAS").await;
    judge(pool, &first, 40.0, "a_verifier").await;
    judge(pool, &second, 85.0, "fiable").await;
    judge(pool, &elsewhere, 20.0, "ecarte").await;
    Group {
        seed,
        profile,
        other_profile,
        first,
        second,
        elsewhere,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_exposes_the_company_entity_and_its_sibling_leads(pool: PgPool) {
    let group = group_fixture(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api.get(&format!("/api/v1/leads/{}", group.first)).await;

    assert_eq!(status, StatusCode::OK);
    let company = &body["data"]["company"];
    assert_eq!(company["siren"], "552081317");
    assert_eq!(company["legal_name"], "ACME SAS");
    let siblings = company["siblings"].as_array().expect("siblings");
    assert_eq!(siblings.len(), 2);
    assert_eq!(siblings[0]["id"], group.second);
    assert_eq!(siblings[0]["domain"], "acme.fr");
    assert_eq!(siblings[0]["profile"]["id"], group.profile);
    assert_eq!(siblings[0]["profile"]["name"], "keycloak-ops");
    assert_eq!(siblings[0]["verdict"], "fiable");
    assert_eq!(siblings[0]["confidence"], 85.0);
    assert_eq!(siblings[1]["id"], group.elsewhere);
    assert_eq!(siblings[1]["profile"]["id"], group.other_profile);
    assert_eq!(siblings[1]["verdict"], "ecarte");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_lead_without_a_siren_has_no_siblings(pool: PgPool) {
    let group = group_fixture(&pool).await;
    let solo = group
        .seed
        .lead(&group.profile, "bare.fr", "new", 1.0, 1)
        .await;
    seed_dossier(&pool, "bare.fr", "partial", false).await;
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{solo}")).await;

    assert!(body["data"]["company"]["legal_name"].is_null());
    assert_eq!(body["data"]["company"]["siblings"], json!([]));
    assert_eq!(body["data"]["facts"]["sibling_domains"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_facts_count_the_signals_of_siblings_of_the_same_profile_once(pool: PgPool) {
    let group = group_fixture(&pool).await;
    let seed = &group.seed;
    seed.host_signal(
        &group.first,
        "saas_provider",
        "auth0",
        "dns_cname",
        "sso.acme.com",
    )
    .await;
    seed.host_signal(
        &group.second,
        "saas_provider",
        "auth0",
        "dns_cname",
        "sso.acme.fr",
    )
    .await;
    seed.host_signal(&group.second, "job_offer", "devops", "ats", "jobs.acme.fr")
        .await;
    seed.host_signal(
        &group.elsewhere,
        "public_tender",
        "iam",
        "boamp",
        "x.acme.com",
    )
    .await;
    let api = Api::new(pool).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{}", group.first)).await;

    let facts = &body["data"]["facts"];
    assert_eq!(facts["signal_count"], 3);
    assert_eq!(facts["independent_signals"], 2);
    assert_eq!(facts["sibling_domains"], json!(["acme.fr"]));
    assert_eq!(facts["sources"], json!(["ats", "dns_cname"]));
    assert_eq!(facts["kinds"], json!(["job_offer", "saas_provider"]));
    assert_eq!(facts["distinct_hosts"], 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_groups_siblings_of_a_company_when_asked(pool: PgPool) {
    let group = group_fixture(&pool).await;
    let api = Api::new(pool).await;

    let (status, body) = api
        .get(&format!(
            "/api/v1/profiles/{}/leads?group_by=company&sort=confidence_desc",
            group.profile
        ))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(domains(&body), vec!["acme.fr", "solo.io"]);
    assert_eq!(body["pagination"]["total"], 2);
    let rows = body["data"].as_array().expect("rows");
    assert_eq!(rows[0]["id"], group.second);
    let grouped = rows[0]["grouped_leads"].as_array().expect("grouped");
    assert_eq!(grouped.len(), 1);
    assert_eq!(grouped[0]["id"], group.first);
    assert_eq!(grouped[0]["domain"], "acme.com");
    assert_eq!(grouped[0]["verdict"], "a_verifier");
    assert_eq!(grouped[0]["confidence"], 40.0);
    assert_eq!(rows[1]["grouped_leads"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_is_flat_by_default_and_rejects_an_unknown_grouping(pool: PgPool) {
    let group = group_fixture(&pool).await;
    let api = Api::new(pool).await;

    let (_, flat) = api
        .get(&format!("/api/v1/profiles/{}/leads", group.profile))
        .await;
    let (_, none) = api
        .get(&format!(
            "/api/v1/profiles/{}/leads?group_by=none",
            group.profile
        ))
        .await;
    let (status, _) = api
        .get(&format!(
            "/api/v1/profiles/{}/leads?group_by=nonsense",
            group.profile
        ))
        .await;

    assert_eq!(flat["pagination"]["total"], 3);
    assert_eq!(flat, none);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(flat["data"][0]["grouped_leads"], json!([]));
}

async fn seed_retry(pool: &PgPool, lead: &str, kind: &str, attempts: i32, error: &str) {
    sqlx::query(
        "INSERT INTO lead_agent_retries (lead_id, kind, profile_revision, agent_id, attempts, next_attempt_at, error, updated_at)
         VALUES ($1::uuid, $2, 1, 'pertinence', $3, now() + interval '6 hours', $4, now())",
    )
    .bind(lead)
    .bind(kind)
    .bind(attempts)
    .bind(error)
    .execute(pool)
    .await
    .expect("retry");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_reports_failing_agents_and_the_retry_resets_the_failed_ones(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 5.0, 1).await;
    seed_retry(
        &pool,
        &lead,
        "scoring",
        5,
        "Échec de l'appel au modèle : status 400",
    )
    .await;
    seed_retry(
        &pool,
        &lead,
        "qualification",
        2,
        "Échec de l'appel au modèle : timeout",
    )
    .await;
    let api = Api::new(pool.clone()).await;

    let (_, body) = api.get(&format!("/api/v1/leads/{lead}")).await;
    let failures = body["data"]["agent_failures"]
        .as_array()
        .unwrap_or_else(|| panic!("{body}"));
    assert_eq!(failures.len(), 2);
    let scoring = failures
        .iter()
        .find(|f| f["kind"] == "scoring")
        .expect("scoring");
    assert_eq!(scoring["attempts"], 5);
    assert_eq!(scoring["failed"], true);
    assert_eq!(scoring["agent_id"], "pertinence");
    assert_eq!(scoring["error"], "Échec de l'appel au modèle : status 400");
    assert!(scoring["next_attempt_at"].is_string());
    let qualification = failures
        .iter()
        .find(|f| f["kind"] == "qualification")
        .expect("qualification");
    assert_eq!(qualification["failed"], false);
    assert_eq!(qualification["agent_name"], "Pertinence");

    let uri = format!("/api/v1/leads/{lead}/agents/retry");
    let (status, _, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(status, StatusCode::OK);

    let rows: Vec<(String, i32, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        "SELECT kind, attempts, next_attempt_at FROM lead_agent_retries ORDER BY kind",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows[0].0, "qualification");
    assert_eq!(rows[0].1, 2);
    assert!(rows[0].2.is_some());
    assert_eq!(rows[1].0, "scoring");
    assert_eq!(rows[1].1, 0);
    assert!(rows[1].2.is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_agents_retry_is_for_users_and_for_known_visible_leads_only(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 5.0, 1).await;
    seed_retry(&pool, &lead, "scoring", 5, "x").await;
    let api = Api::new(pool.clone()).await;
    let client = format!("Bearer {}", api.harness.valid_client_token());

    let uri = format!("/api/v1/leads/{lead}/agents/retry");
    let (forbidden, _, _) = send(api.app(), Method::POST, &uri, Some(&client), None).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = send(api.app(), Method::POST, &uri, None, None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM lead_agent_retries")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(attempts, 5);

    let uri = format!("/api/v1/leads/{}/agents/retry", common::generate_uuid_v7());
    let (missing, _, _) = send(api.app(), Method::POST, &uri, Some(&api.bearer), None).await;
    assert_eq!(missing, StatusCode::NOT_FOUND);
    let (malformed, _, _) = send(
        api.app(),
        Method::POST,
        "/api/v1/leads/nope/agents/retry",
        Some(&api.bearer),
        None,
    )
    .await;
    assert_eq!(malformed, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_exposes_the_enrichment_steps_status_and_waiting_reason(pool: PgPool) {
    let seed = Seed { pool: pool.clone() };
    let profile = seed.qualified_profile("keycloak-ops").await;
    let lead = seed.lead(&profile, "acme.com", "new", 3.0, 1).await;
    seed.host_signal(
        &lead,
        "certificate_seen",
        "sso.acme.com",
        "certificate_transparency",
        "sso.acme.com",
    )
    .await;
    let api = Api::new(pool.clone()).await;
    let uri = format!("/api/v1/leads/{lead}");

    let (status, body) = api.get(&uri).await;
    assert_eq!(status, StatusCode::OK);
    let enrichment = &body["data"]["enrichment"];
    assert_eq!(enrichment["status"], "pending");
    assert_eq!(enrichment["waiting_reason"], "awaiting_enrichment_evidence");
    assert!(enrichment["wait_until"].is_string());
    assert_eq!(
        enrichment["steps"],
        json!([
            {"step": "http_probe", "enabled": false, "ran_at": null},
            {"step": "company_jobs", "enabled": true, "ran_at": null}
        ])
    );

    query(
        "INSERT INTO lead_enrichments (lead_id, step, ran_at) VALUES ($1::uuid, 'company_jobs', now())",
    )
    .bind(&lead)
    .execute(&pool)
    .await
    .unwrap();
    let (_, body) = api.get(&uri).await;
    let enrichment = &body["data"]["enrichment"];
    assert_eq!(enrichment["status"], "complete");
    assert_eq!(enrichment["waiting_reason"], "awaiting_enrichment_evidence");
    assert!(enrichment["wait_until"].is_null());
    assert!(enrichment["steps"][1]["ran_at"].is_string());

    seed.signal(&lead, "product_detected").await;
    let (_, body) = api.get(&uri).await;
    assert!(body["data"]["enrichment"]["waiting_reason"].is_null());
}
