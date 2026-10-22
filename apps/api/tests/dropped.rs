mod support;

use std::slice::from_ref;

use api::openapi::ApiDoc;
use common::generate_uuid_v7;
use http::{Method, StatusCode};
use lodestone_core::{CreateProfileCommand, ProfileName, ValidatedProfile};
use serde_json::{Value, json};
use sqlx::{PgPool, query, query_scalar};
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

    async fn review(&self, id: &str, body: Value) -> (StatusCode, Value) {
        let uri = format!("/api/v1/dropped/{id}/review");
        let (status, body, _) = send(
            self.harness.public(),
            Method::POST,
            &uri,
            Some(&self.bearer),
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
            "keywords": ["IAM"],
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

    async fn dropped(
        &self,
        profile: &str,
        host: &str,
        agent: Option<&str>,
        reason: &str,
        age_minutes: i32,
    ) -> String {
        let id = generate_uuid_v7();
        query(
            "INSERT INTO dropped_candidates
                 (id, profile_id, agent_id, source, kind, domain, title, excerpts, provenance,
                  observed_at, reason, matched, excluded, details, dropped_at)
             VALUES ($1, $2::uuid, $3, 'boamp', 'job_offer', $4, $5, $6, $7,
                     now(), $8, $9, $10, '{\"id\": 1}'::jsonb, now() - make_interval(mins => $11))",
        )
        .bind(id)
        .bind(profile)
        .bind(agent)
        .bind(host)
        .bind(format!("Offre {host}"))
        .bind(json!(["passage un", "passage deux"]))
        .bind(format!("https://jobs.example/{host}"))
        .bind(reason)
        .bind(json!(["IAM"]))
        .bind(json!(["stagiaire"]))
        .bind(age_minutes)
        .execute(&self.pool)
        .await
        .expect("dropped candidate");
        id.to_string()
    }

    async fn seeded(&self) -> (String, [String; 3]) {
        let profile = self.profile("iam").await;
        let first = self
            .dropped(&profile, "a.fr", Some("jobs"), "no_keyword", 30)
            .await;
        let second = self
            .dropped(&profile, "b.fr", Some("jobs"), "exclusion", 20)
            .await;
        let third = self
            .dropped(&profile, "c.fr", Some("other"), "filter_remote", 10)
            .await;
        (profile, [first, second, third])
    }
}

fn ids(body: &Value) -> Vec<String> {
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_returns_the_dropped_candidates_newest_first_with_their_shape(pool: PgPool) {
    let api = Api::new(pool).await;
    let (profile, [first, second, third]) = api.seeded().await;

    let (status, body) = api
        .get(&format!("/api/v1/profiles/{profile}/dropped"))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ids(&body), [third.clone(), second, first]);
    let row = &body["data"][0];
    assert_eq!(row["id"], third.as_str());
    assert_eq!(row["profile_id"], profile.as_str());
    assert_eq!(row["agent"], "other");
    assert_eq!(row["source"], "boamp");
    assert_eq!(row["kind"], "job_offer");
    assert_eq!(row["domain"], "c.fr");
    assert_eq!(row["title"], "Offre c.fr");
    assert_eq!(row["excerpts"], json!(["passage un", "passage deux"]));
    assert_eq!(row["provenance"], "https://jobs.example/c.fr");
    assert_eq!(row["reason"], "filter_remote");
    assert_eq!(row["matched"], json!(["IAM"]));
    assert_eq!(row["excluded"], json!(["stagiaire"]));
    assert!(row["observed_at"].is_string());
    assert!(row["dropped_at"].is_string());
    assert_eq!(row["review"], Value::Null);
    assert_eq!(row["reviewed_at"], Value::Null);
    assert!(row.get("details").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_filters_by_reason_agent_and_review_and_honours_the_limit(pool: PgPool) {
    let api = Api::new(pool).await;
    let (profile, [first, second, third]) = api.seeded().await;
    let base = format!("/api/v1/profiles/{profile}/dropped");

    let (_, by_reason) = api.get(&format!("{base}?reason=exclusion")).await;
    assert_eq!(ids(&by_reason), from_ref(&second));
    let (_, by_agent) = api.get(&format!("{base}?agent=jobs")).await;
    assert_eq!(ids(&by_agent), [second.clone(), first.clone()]);
    let (_, limited) = api.get(&format!("{base}?limit=2")).await;
    assert_eq!(ids(&limited), [third.clone(), second.clone()]);
    let (_, sample) = api.get(&format!("{base}?order=random&limit=2")).await;
    assert_eq!(ids(&sample).len(), 2);
    let (_, recent) = api.get(&format!("{base}?order=recent&limit=1")).await;
    assert_eq!(ids(&recent), from_ref(&third));
    let (_, pending) = api.get(&format!("{base}?reviewed=false")).await;
    assert_eq!(ids(&pending).len(), 3);
    let (_, done) = api.get(&format!("{base}?reviewed=true")).await;
    assert!(ids(&done).is_empty());

    let (status, _) = api
        .review(&first, json!({"review": "rightly_dropped"}))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, done) = api.get(&format!("{base}?reviewed=true")).await;
    assert_eq!(ids(&done), from_ref(&first));
    assert_eq!(done["data"][0]["review"], "rightly_dropped");
    assert!(done["data"][0]["reviewed_at"].is_string());
    let (_, pending) = api.get(&format!("{base}?reviewed=false")).await;
    assert_eq!(ids(&pending), [third, second]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_rejects_invalid_parameters(pool: PgPool) {
    let api = Api::new(pool).await;
    let (profile, _) = api.seeded().await;
    let base = format!("/api/v1/profiles/{profile}/dropped");

    for query in [
        "reason=nothing",
        "reviewed=maybe",
        "order=sideways",
        "limit=0",
        "limit=101",
        "limit=many",
        "limit=-1",
        "agent=Not%20An%20Agent",
    ] {
        let (status, body) = api.get(&format!("{base}?{query}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert_eq!(body["code"], "E_VALIDATION", "{query}");
    }
    let (status, _) = api.get("/api/v1/profiles/not-a-uuid/dropped").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = api.get(&format!("{base}?limit=100")).await;
    assert_eq!(status, StatusCode::OK);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_of_an_unknown_profile_is_not_found(pool: PgPool) {
    let api = Api::new(pool).await;
    let uri = format!("/api/v1/profiles/{}/dropped", generate_uuid_v7());
    let (status, body) = api.get(&uri).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "E_NOT_FOUND");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_is_closed_to_clients_and_anonymous_callers(pool: PgPool) {
    let api = Api::new(pool).await;
    let (profile, _) = api.seeded().await;
    let uri = format!("/api/v1/profiles/{profile}/dropped");
    let client = format!("Bearer {}", api.harness.valid_client_token());

    let (forbidden, _, _) =
        send(api.harness.public(), Method::GET, &uri, Some(&client), None).await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = send(api.harness.public(), Method::GET, &uri, None, None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn recovering_a_candidate_returns_the_lead_and_a_second_review_conflicts(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let (profile, [first, ..]) = api.seeded().await;

    let (status, body) = api
        .review(&first, json!({"review": "should_have_kept"}))
        .await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["candidate"]["id"], first.as_str());
    assert_eq!(data["candidate"]["review"], "should_have_kept");
    assert!(data["candidate"]["reviewed_at"].is_string());
    let lead = data["lead_id"].as_str().unwrap();
    let stored: String =
        query_scalar("SELECT domain FROM leads WHERE id = $1::uuid AND profile_id = $2::uuid")
            .bind(lead)
            .bind(&profile)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, "a.fr");
    let route: String = query_scalar(
        "SELECT raw->>'route' FROM signals WHERE lead_id = $1::uuid AND agent_id = 'jobs'",
    )
    .bind(lead)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(route, "recovered");

    for review in ["should_have_kept", "rightly_dropped"] {
        let (status, body) = api.review(&first, json!({"review": review})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{review}");
        assert_eq!(body["code"], "E_CONFLICT");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn confirming_a_drop_returns_no_lead(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let (_, [first, ..]) = api.seeded().await;

    let (status, body) = api
        .review(&first, json!({"review": "rightly_dropped"}))
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["candidate"]["review"], "rightly_dropped");
    assert_eq!(body["data"]["lead_id"], Value::Null);
    let leads: i64 = query_scalar("SELECT COUNT(*) FROM leads")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(leads, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn recovering_under_an_inactive_profile_conflicts(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let (profile, [first, ..]) = api.seeded().await;
    query("UPDATE profiles SET active = false WHERE id = $1::uuid")
        .bind(&profile)
        .execute(&pool)
        .await
        .unwrap();

    let (status, body) = api
        .review(&first, json!({"review": "should_have_kept"}))
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "E_CONFLICT");
    let (_, listed) = api
        .get(&format!(
            "/api/v1/profiles/{profile}/dropped?reviewed=false"
        ))
        .await;
    assert_eq!(ids(&listed).len(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_review_rejects_unknown_candidates_bad_identifiers_and_bad_bodies(pool: PgPool) {
    let api = Api::new(pool).await;
    let (_, [first, ..]) = api.seeded().await;

    let (status, _) = api
        .review(
            &generate_uuid_v7().to_string(),
            json!({"review": "rightly_dropped"}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = api
        .review("not-a-uuid", json!({"review": "rightly_dropped"}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for body in [
        json!({"review": "maybe"}),
        json!({"review": null}),
        json!({}),
        json!("should_have_kept"),
    ] {
        let (status, _) = api.review(&first, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, _, _) = send(
        api.harness.public(),
        Method::POST,
        &format!("/api/v1/dropped/{first}/review"),
        Some(&api.bearer),
        None,
    )
    .await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_review_is_closed_to_clients_and_anonymous_callers(pool: PgPool) {
    let api = Api::new(pool).await;
    let (_, [first, ..]) = api.seeded().await;
    let uri = format!("/api/v1/dropped/{first}/review");
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let body = json!({"review": "rightly_dropped"});

    let (forbidden, _, _) = send(
        api.harness.public(),
        Method::POST,
        &uri,
        Some(&client),
        Some(body.clone()),
    )
    .await;
    assert_eq!(forbidden, StatusCode::FORBIDDEN);
    let (anonymous, _, _) = send(api.harness.public(), Method::POST, &uri, None, Some(body)).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_quality_report_lists_the_triage_recall(pool: PgPool) {
    let api = Api::new(pool).await;
    let (profile, [first, second, _]) = api.seeded().await;
    let extra = api
        .dropped(&profile, "d.fr", Some("jobs"), "no_keyword", 5)
        .await;
    api.review(&first, json!({"review": "should_have_kept"}))
        .await;
    api.review(&extra, json!({"review": "rightly_dropped"}))
        .await;
    api.review(&second, json!({"review": "rightly_dropped"}))
        .await;

    let (status, body) = api
        .get(&format!("/api/v1/quality?profile_id={profile}"))
        .await;

    assert_eq!(status, StatusCode::OK);
    let recall = body["data"]["triage_recall"].as_array().unwrap();
    assert_eq!(recall.len(), 3);
    let row = |agent: &str, reason: &str| {
        recall
            .iter()
            .find(|row| row["agent"] == agent && row["reason"] == reason)
            .unwrap_or_else(|| panic!("no recall row for {agent} {reason}"))
    };
    let no_keyword = row("jobs", "no_keyword");
    assert_eq!(no_keyword["profile_id"], profile.as_str());
    assert_eq!(no_keyword["dropped"], 2);
    assert_eq!(no_keyword["reviewed"], 2);
    assert_eq!(no_keyword["should_have_kept"], 1);
    assert_eq!(no_keyword["share"], 0.5);
    let exclusion = row("jobs", "exclusion");
    assert_eq!(exclusion["reviewed"], 1);
    assert_eq!(exclusion["share"], 0.0);
    let pending = row("other", "filter_remote");
    assert_eq!(pending["dropped"], 1);
    assert_eq!(pending["reviewed"], 0);
    assert_eq!(pending["should_have_kept"], 0);
    assert_eq!(pending["share"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_quality_recall_follows_the_period_and_is_empty_without_drops(pool: PgPool) {
    let api = Api::new(pool).await;
    let (status, body) = api.get("/api/v1/quality").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["triage_recall"], json!([]));

    let profile = api.profile("iam").await;
    api.dropped(&profile, "old.fr", Some("jobs"), "no_keyword", 60 * 24 * 10)
        .await;
    api.dropped(&profile, "new.fr", Some("jobs"), "no_keyword", 5)
        .await;
    let from = (chrono::Utc::now() - chrono::Duration::days(2))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

    let (_, all) = api
        .get(&format!("/api/v1/quality?profile_id={profile}"))
        .await;
    assert_eq!(all["data"]["triage_recall"][0]["dropped"], 2);
    let (_, recent) = api
        .get(&format!("/api/v1/quality?profile_id={profile}&from={from}"))
        .await;
    assert_eq!(recent["data"]["triage_recall"][0]["dropped"], 1);
}

#[test]
fn openapi_documents_the_dropped_paths_and_the_recall() {
    let spec = serde_json::to_value(ApiDoc::openapi()).expect("spec");
    for (path, method) in [
        ("/api/v1/profiles/{profile_id}/dropped", "get"),
        ("/api/v1/dropped/{dropped_id}/review", "post"),
    ] {
        assert!(spec["paths"][path][method].is_object(), "{method} {path}");
    }
    let schemas = &spec["components"]["schemas"];
    for name in [
        "DroppedCandidateDto",
        "ReviewedDropDto",
        "ReviewDroppedRequest",
        "DropReasonDto",
        "DropReviewDto",
        "TriageRecallDto",
    ] {
        assert!(schemas[name].is_object(), "{name}");
    }
    assert!(schemas["QualityDto"]["properties"]["triage_recall"].is_object());
}
