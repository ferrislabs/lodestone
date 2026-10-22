mod support;

use api::openapi::ApiDoc;
use common::generate_uuid_v7;
use http::{Method, StatusCode};
use lodestone_core::{CreateProfileCommand, ProfileName, ValidatedProfile};
use serde_json::{Value, json};
use sqlx::{PgPool, query};
use support::{Harness, send};
use utoipa::OpenApi;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct Api {
    harness: Harness,
    bearer: String,
    pool: PgPool,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool.clone()).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self {
            harness,
            bearer,
            pool,
        }
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        let (status, body, _) = send(
            self.harness.public(),
            Method::GET,
            uri,
            Some(&self.bearer),
            None,
        )
        .await;
        (status, body)
    }

    async fn apply(&self, profile: &str, body: Value) -> (StatusCode, Value) {
        self.apply_as(&self.bearer, profile, body).await
    }

    async fn apply_as(&self, bearer: &str, profile: &str, body: Value) -> (StatusCode, Value) {
        let uri = format!("/api/v1/profiles/{profile}/keyword-suggestions/apply");
        let (status, body, _) = send(
            self.harness.public(),
            Method::POST,
            &uri,
            Some(bearer),
            Some(body),
        )
        .await;
        (status, body)
    }

    async fn profile(&self, name: &str) -> String {
        let mut definition: Value = serde_json::from_str(IAM).unwrap();
        definition["agents"] = json!([{
            "type": "search",
            "id": "jobs",
            "name": "Jobs",
            "enabled": true,
            "providers": ["boamp"],
            "keywords": ["sso", "mfa"],
            "exclusions": ["freelance"],
            "accept_min_keywords": 1,
        }]);
        let usecase = &self.harness.state.usecase;
        let id = usecase
            .create_profile(CreateProfileCommand {
                name: ProfileName::parse(name).unwrap(),
                definition: ValidatedProfile::parse(definition).unwrap(),
                created_by: None,
            })
            .await
            .unwrap()
            .profile
            .id;
        usecase.set_profile_active(id, true).await.unwrap();
        id.to_string()
    }

    async fn found(
        &self,
        profile: &str,
        host: &str,
        matched: &[&str],
        title: &str,
        decision: Option<&str>,
    ) {
        let lead = generate_uuid_v7();
        query(
            "INSERT INTO leads (id, profile_id, domain, status, score, verdict, first_seen_at, last_seen_at, updated_at)
             VALUES ($1, $2::uuid, $3, 'new', 1, 'a_verifier', now(), now(), now())",
        )
        .bind(lead)
        .bind(profile)
        .bind(host)
        .execute(&self.pool)
        .await
        .expect("lead");
        let signal = generate_uuid_v7();
        query(
            "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint, agent_id)
             VALUES ($1, $2, 'job_offer', $3, 'boamp', $4, $5, 0.8, now(), $1::text, 'jobs')",
        )
        .bind(signal)
        .bind(lead)
        .bind(title)
        .bind(json!({"title": title, "matched": matched, "route": "accept"}))
        .bind(format!("https://jobs.example/{host}"))
        .execute(&self.pool)
        .await
        .expect("signal");
        if let Some(kind) = decision {
            query(
                "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
                 VALUES ($1, $2, $3::uuid, 1, $4, now(), $5)",
            )
            .bind(generate_uuid_v7())
            .bind(lead)
            .bind(profile)
            .bind(kind)
            .bind(json!({
                "confidence": null,
                "opinion": null,
                "evidence": null,
                "verdict": "a_verifier",
                "verdict_reason": null,
                "agents": {},
            }))
            .execute(&self.pool)
            .await
            .expect("label");
        }
    }

    async fn seeded(&self) -> String {
        let profile = self.profile("yield").await;
        for index in 0..5 {
            self.found(
                &profile,
                &format!("sso{index}.fr"),
                &["sso"],
                "Stage ingénieur",
                Some("lead_rejected"),
            )
            .await;
        }
        self.found(
            &profile,
            "mfa.fr",
            &["mfa"],
            "Ingénieur MFA",
            Some("lead_validated"),
        )
        .await;
        self.found(&profile, "open.fr", &["mfa", "sso"], "Architecte", None)
            .await;
        profile
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_empty_database_has_no_yield_and_no_suggestion(pool: PgPool) {
    let api = Api::new(pool).await;
    let (status, body) = api.get("/api/v1/quality").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["sourcing_yield"], json!([]));
    assert_eq!(body["data"]["keyword_suggestions"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_quality_page_exposes_the_yield_and_the_suggestions(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;

    let (status, body) = api
        .get(&format!("/api/v1/quality?profile_id={profile}"))
        .await;

    assert_eq!(status, StatusCode::OK);
    let agents = body["data"]["sourcing_yield"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    let jobs = &agents[0];
    assert_eq!(jobs["profile_id"], json!(profile));
    assert_eq!(jobs["agent_id"], "jobs");
    assert_eq!(jobs["agent_name"], "Jobs");
    assert_eq!(jobs["leads"], 7);
    assert_eq!(jobs["validated"], 1);
    assert_eq!(jobs["rejected"], 5);
    assert_eq!(jobs["precision"], json!(1.0 / 6.0));
    assert_eq!(
        jobs["verdicts"],
        json!({"reliable": 0, "to_verify": 7, "discarded": 0, "none": 0})
    );
    let keywords = jobs["keywords"].as_array().unwrap();
    assert_eq!(keywords[0]["keyword"], "sso");
    assert_eq!(keywords[0]["leads"], 6);
    assert_eq!(keywords[0]["exclusive_leads"], 5);
    assert_eq!(keywords[0]["validated"], 0);
    assert_eq!(keywords[0]["rejected"], 5);
    assert_eq!(keywords[0]["precision"], json!(0.0));
    assert_eq!(keywords[1]["keyword"], "mfa");
    assert_eq!(keywords[1]["leads"], 2);
    assert_eq!(keywords[1]["exclusive_leads"], 1);
    assert_eq!(keywords[1]["precision"], json!(1.0));

    let suggestions = body["data"]["keyword_suggestions"].as_array().unwrap();
    assert_eq!(
        suggestions[0],
        json!({
            "profile_id": profile,
            "agent_id": "jobs",
            "action": "remove_keyword",
            "word": "sso",
            "decided": 5,
            "rejected": 5,
        })
    );
    assert_eq!(
        suggestions[1],
        json!({
            "profile_id": profile,
            "agent_id": "jobs",
            "action": "add_exclusion",
            "word": "stage",
            "decided": 5,
            "rejected": 5,
        })
    );
    assert_eq!(suggestions.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn applying_a_suggestion_returns_the_new_revision(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;

    let (status, body) = api
        .apply(
            &profile,
            json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 1}),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["id"], json!(profile));
    assert_eq!(body["data"]["revision"], 2);
    assert_eq!(
        body["data"]["definition"]["agents"][0]["keywords"],
        json!(["mfa"])
    );
    let (_, current) = api.get(&format!("/api/v1/profiles/{profile}")).await;
    assert_eq!(current["data"]["revision"], 2);
    let (_, listing) = api
        .get(&format!("/api/v1/profiles/{profile}/revisions"))
        .await;
    assert_eq!(listing["data"].as_array().unwrap().len(), 2);

    let (status, body) = api
        .apply(
            &profile,
            json!({"agent_id": "jobs", "action": "add_exclusion", "word": "stage", "revision": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["revision"], 3);
    assert_eq!(
        body["data"]["definition"]["agents"][0]["exclusions"],
        json!(["freelance", "stage"])
    );

    let (_, report) = api
        .get(&format!("/api/v1/quality?profile_id={profile}"))
        .await;
    let suggestions = report["data"]["keyword_suggestions"].as_array().unwrap();
    assert!(suggestions.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suggestion_read_against_an_older_revision_is_a_conflict(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;
    let (first, _) = api
        .apply(
            &profile,
            json!({"agent_id": "jobs", "action": "add_exclusion", "word": "stage", "revision": 1}),
        )
        .await;
    assert_eq!(first, StatusCode::OK);

    let (status, body) = api
        .apply(
            &profile,
            json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 1}),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "E_CONFLICT");
    let (_, listing) = api
        .get(&format!("/api/v1/profiles/{profile}/revisions"))
        .await;
    assert_eq!(listing["data"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suggestion_that_no_longer_applies_is_a_conflict(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;
    for (action, word) in [("remove_keyword", "scim"), ("add_exclusion", "freelance")] {
        let (status, body) = api
            .apply(
                &profile,
                json!({"agent_id": "jobs", "action": action, "word": word, "revision": 1}),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{action} {word}");
        assert_eq!(body["code"], "E_CONFLICT");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_unknown_profile_or_agent_is_not_found(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;

    let (status, body) = api
        .apply(
            &generate_uuid_v7().to_string(),
            json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "E_NOT_FOUND");

    let (status, body) = api
        .apply(
            &profile,
            json!({"agent_id": "ghost", "action": "remove_keyword", "word": "sso", "revision": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "E_NOT_FOUND");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_malformed_request_is_refused(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;

    let (status, _) = api
        .apply(
            "not-a-uuid",
            json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for body in [
        json!({"agent_id": "jobs", "action": "rename", "word": "sso", "revision": 1}),
        json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso"}),
        json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 0}),
    ] {
        let (status, _) = api.apply(&profile, body.clone()).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
            "{body}: {status}"
        );
    }
    let (status, _) = api
        .apply(
            &profile,
            json!({"agent_id": "jobs", "action": "add_exclusion", "word": "  ", "revision": 1}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_apply_is_closed_to_clients_and_anonymous_callers(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.seeded().await;
    let body =
        json!({"agent_id": "jobs", "action": "remove_keyword", "word": "sso", "revision": 1});
    let client = format!("Bearer {}", api.harness.valid_client_token());

    let (forbidden, _) = api.apply_as(&client, &profile, body.clone()).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let uri = format!("/api/v1/profiles/{profile}/keyword-suggestions/apply");
    let (anonymous, _, _) = send(api.harness.public(), Method::POST, &uri, None, Some(body)).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);

    let (_, listing) = api
        .get(&format!("/api/v1/profiles/{profile}/revisions"))
        .await;
    assert_eq!(listing["data"].as_array().unwrap().len(), 1);
}

#[test]
fn openapi_documents_the_apply_path_and_the_yield() {
    let spec = serde_json::to_value(ApiDoc::openapi()).expect("spec");
    assert!(
        spec["paths"]["/api/v1/profiles/{profile_id}/keyword-suggestions/apply"]["post"]
            .is_object()
    );
    let schemas = &spec["components"]["schemas"];
    for name in [
        "AgentYieldDto",
        "KeywordYieldDto",
        "KeywordSuggestionDto",
        "SuggestionActionDto",
        "ApplyKeywordSuggestionRequest",
    ] {
        assert!(schemas[name].is_object(), "{name}");
    }
    assert!(schemas["QualityDto"]["properties"]["sourcing_yield"].is_object());
    assert!(schemas["QualityDto"]["properties"]["keyword_suggestions"].is_object());
}
