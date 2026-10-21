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

    assert_eq!(data["score_breakdown"][0]["signal"], "product_detected");
    assert_eq!(data["score_breakdown"][0]["value"], "keycloak");
    assert_eq!(data["score_breakdown"][0]["contribution"], 8.0);

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
    "schema_version": 4,
    "identity": {"offer": "Managed IAM", "target_customer": "Companies running Keycloak"},
    "agents": [{"id": "ats-a", "name": "ATS", "source": {"kind": "ats", "keywords": ["sso"]}}],
    "qualification_agents": [
        {"id": "pertinence", "name": "Pertinence", "system_prompt": "p", "weight": 5,
         "questions": [{"id": "pertinence", "wording": "Pertinent ?", "weight": 5,
            "options": [
                {"value": "forte", "label": "Forte", "score": 1.0},
                {"value": "moyenne", "label": "Moyenne", "score": 0.5},
                {"value": "faible", "label": "Faible", "score": 0.0}]}]},
        {"id": "maturite", "name": "Maturité", "system_prompt": "m", "weight": 5,
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
            answers,
            score,
            error,
        } = seeded;
        sqlx::query(
            "INSERT INTO qualification_results (id, lead_id, profile_id, profile_revision, agent_id, provider_id, model, prompt_hash, status, answers, agent_score, error, created_at)
             SELECT $1, l.id, l.profile_id, 2, $3, $4::uuid, 'gemma_laya', 'hash', $5, $6, $7, $8, now()
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
    assert_eq!(body["data"]["lead"]["summary"], "s");
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
            answers: json!({"pertinence": {"type": "choice", "choice": "moyenne", "confidence": 0.8,
                "probabilities": {"forte": 0.25, "moyenne": 0.5, "faible": 0.25}}}),
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
    assert_eq!(first["provider_name"], "Laya");
    assert_eq!(first["provider_id"], provider);
    assert_eq!(first["model"], "gemma_laya");
    assert_eq!(first["status"], "valid");
    assert_eq!(first["agent_score"], 0.5);
    assert!(first["error"].is_null());
    let answer = &first["answers"][0];
    assert_eq!(answer["question_id"], "pertinence");
    assert_eq!(answer["wording"], "Pertinent ?");
    assert_eq!(answer["choice"], "moyenne");
    assert_eq!(answer["choice_label"], "Moyenne");
    assert_eq!(answer["confidence"], 0.8);
    assert_eq!(answer["probabilities"][0]["value"], "forte");
    assert_eq!(answer["probabilities"][0]["label"], "Forte");
    assert_eq!(answer["probabilities"][0]["probability"], 0.25);
    assert_eq!(answer["probabilities"][1]["probability"], 0.5);
    let invalid = &results[1];
    assert_eq!(invalid["agent_id"], "maturite");
    assert_eq!(invalid["agent_name"], "Maturité");
    assert_eq!(invalid["status"], "invalid");
    assert!(invalid["agent_score"].is_null());
    assert_eq!(invalid["answers"].as_array().unwrap().len(), 0);
    assert!(invalid["error"].as_str().unwrap().contains("maturite"));
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
