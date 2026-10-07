mod support;

use http::{Method, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::{Harness, send};
use utoipa::OpenApi;

async fn profile(pool: &PgPool, name: &str) -> String {
    let id = common::generate_uuid_v7();
    sqlx::query(
        "INSERT INTO profiles (id, name, active, created_at, updated_at) VALUES ($1, $2, true, now(), now())",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .expect("profile");
    id.to_string()
}

async fn lead(pool: &PgPool, profile: &str, domain: &str, status: &str) -> String {
    let id = common::generate_uuid_v7();
    sqlx::query(
        "INSERT INTO leads (id, profile_id, domain, status, score, score_breakdown, first_seen_at, last_seen_at, updated_at)
         VALUES ($1, $2::uuid, $3, $4, 1, '[]'::jsonb, now(), now(), now())",
    )
    .bind(id)
    .bind(profile)
    .bind(domain)
    .bind(status)
    .execute(pool)
    .await
    .expect("lead");
    id.to_string()
}

async fn label(
    pool: &PgPool,
    lead: &str,
    profile: &str,
    kind: &str,
    confidence: f64,
    verdict: &str,
    days_ago: i32,
) {
    sqlx::query(
        "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
         VALUES ($1, $2::uuid, $3::uuid, 1, $4, now() - make_interval(days => $5), $6)",
    )
    .bind(common::generate_uuid_v7())
    .bind(lead)
    .bind(profile)
    .bind(kind)
    .bind(days_ago)
    .bind(json!({
        "confidence": confidence,
        "opinion": null,
        "evidence": null,
        "verdict": verdict,
        "verdict_reason": null,
        "agents": {"fit": {"status": "valid", "score": confidence / 100.0}},
    }))
    .execute(pool)
    .await
    .expect("label");
}

fn provenance(model: &str, prompt: &str) -> Value {
    json!({
        "provider_id": null,
        "provider_name": "local",
        "model": model,
        "prompt_version": prompt,
        "generation": {"temperature": 0.2},
        "samples": 1,
    })
}

async fn versioned_label(
    pool: &PgPool,
    lead: &str,
    profile: &str,
    kind: &str,
    agent_model: &str,
    agent_prompt: &str,
    reason: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot, rejection_reason, rejection_note)
         VALUES ($1, $2::uuid, $3::uuid, 1, $4, now(), $5, $6, $7)",
    )
    .bind(common::generate_uuid_v7())
    .bind(lead)
    .bind(profile)
    .bind(kind)
    .bind(json!({
        "confidence": 60.0,
        "opinion": null,
        "evidence": null,
        "verdict": "fiable",
        "verdict_reason": null,
        "agents": {"fit": {
            "status": "valid",
            "score": 0.6,
            "provenance": provenance(agent_model, agent_prompt),
        }},
        "aggregator": provenance("big", "aggregator@x"),
    }))
    .bind(reason)
    .bind(reason.filter(|reason| *reason == "autre").map(|_| "détail"))
    .execute(pool)
    .await
    .expect("label");
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
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_empty_database_yields_an_empty_report(pool: PgPool) {
    let api = Api::new(pool).await;
    let (status, body) = api.get("/api/v1/quality").await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(
        data["decisions"],
        json!({"validated": 0, "rejected": 0, "drafts_approved": 0, "drafts_rejected": 0})
    );
    assert_eq!(data["confidence"].as_array().unwrap().len(), 10);
    assert_eq!(data["verdicts"].as_array().unwrap().len(), 3);
    assert_eq!(data["verdicts"][0]["precision"], Value::Null);
    assert_eq!(data["discarded"]["share"], Value::Null);
    assert_eq!(data["agents"], json!([]));
    assert_eq!(data["profiles"], json!([]));
    assert_eq!(data["exclusions"], json!([]));
    assert_eq!(data["exclusion_candidates"], json!([]));
}

async fn excluded(
    pool: &PgPool,
    profile: &str,
    domain: &str,
    options: &[(&str, &str, &str)],
) -> String {
    let id = lead(pool, profile, domain, "new").await;
    sqlx::query("UPDATE leads SET confidence = 80, verdict = 'ecarte', qualified_at = now() WHERE id = $1::uuid")
        .bind(&id)
        .execute(pool)
        .await
        .expect("verdict");
    let items: Vec<Value> = options
        .iter()
        .map(|(question, option, label)| {
            json!({
                "agent_id": "exclusions",
                "agent_name": "Exclusions",
                "question_id": question,
                "option": option,
                "label": label,
                "probability": 0.9,
                "evidence": "preuve",
                "evidence_source": "lead"
            })
        })
        .collect();
    sqlx::query(
        "INSERT INTO lead_qualification_state (lead_id, profile_revision, sources, completed_at, scoring_version, evidence, verdict_reason, exclusions)
         VALUES ($1::uuid, 1, ARRAY['dns_cname'], now(), 2, $2, 'excluded', $3)",
    )
    .bind(&id)
    .bind(json!({
        "score": 0.5,
        "components": {"volume": 0.5, "diversity": 0.5, "environment": 0.5, "strength": 0.5},
        "signal_count": 1,
        "signal_score": 1.0,
        "only_non_production": false
    }))
    .bind(Value::Array(items))
    .execute(pool)
    .await
    .expect("state");
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_report_counts_exclusions_per_agent_and_option(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let beta = profile(&pool, "beta").await;
    excluded(
        &pool,
        &alpha,
        "a.com",
        &[("concurrent", "oui", "Concurrent")],
    )
    .await;
    excluded(
        &pool,
        &alpha,
        "b.com",
        &[("concurrent", "oui", "Concurrent")],
    )
    .await;
    excluded(
        &pool,
        &alpha,
        "c.com",
        &[
            ("concurrent", "oui", "Concurrent"),
            ("client_actuel", "oui", "Client actuel"),
        ],
    )
    .await;
    excluded(
        &pool,
        &beta,
        "d.com",
        &[("client_actuel", "oui", "Client actuel")],
    )
    .await;
    let plain = lead(&pool, &alpha, "plain.com", "new").await;
    sqlx::query("UPDATE leads SET verdict = 'fiable', qualified_at = now() WHERE id = $1::uuid")
        .bind(&plain)
        .execute(&pool)
        .await
        .unwrap();
    let api = Api::new(pool).await;

    let (status, body) = api.get("/api/v1/quality").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["exclusions"],
        json!([
            {"agent_id": "exclusions", "agent_name": "Exclusions", "question_id": "concurrent", "option": "oui", "label": "Concurrent", "count": 3},
            {"agent_id": "exclusions", "agent_name": "Exclusions", "question_id": "client_actuel", "option": "oui", "label": "Client actuel", "count": 2},
        ])
    );

    let (_, filtered) = api.get(&format!("/api/v1/quality?profile_id={beta}")).await;
    assert_eq!(
        filtered["data"]["exclusions"],
        json!([
            {"agent_id": "exclusions", "agent_name": "Exclusions", "question_id": "client_actuel", "option": "oui", "label": "Client actuel", "count": 1},
        ])
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejected_leads_with_an_exclusion_reason_become_candidates_for_the_lists(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let rival = lead(&pool, &alpha, "rival.com", "rejected").await;
    versioned_label(
        &pool,
        &rival,
        &alpha,
        "lead_rejected",
        "m",
        "p@1",
        Some("concurrent"),
    )
    .await;
    let off_target = lead(&pool, &alpha, "far.com", "rejected").await;
    versioned_label(
        &pool,
        &off_target,
        &alpha,
        "lead_rejected",
        "m",
        "p@1",
        Some("hors_cible"),
    )
    .await;
    let validated = lead(&pool, &alpha, "kept.com", "validated").await;
    versioned_label(
        &pool,
        &validated,
        &alpha,
        "lead_validated",
        "m",
        "p@1",
        None,
    )
    .await;
    sqlx::query(
        "INSERT INTO company_dossiers (domain, state, siren, official, pages, built_at, attempts, next_attempt_at)
         VALUES ('rival.com', 'complete', '552081317', $1, '[]'::jsonb, now(), 0, now())",
    )
    .bind(json!({"siren": "552081317", "legal_name": "RIVAL SAS"}))
    .execute(&pool)
    .await
    .expect("dossier");
    let api = Api::new(pool).await;

    let (status, body) = api.get("/api/v1/quality").await;

    assert_eq!(status, StatusCode::OK);
    let candidates = body["data"]["exclusion_candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["domain"], "rival.com");
    assert_eq!(candidates[0]["legal_name"], "RIVAL SAS");
    assert_eq!(candidates[0]["reason"], "concurrent");
    assert_eq!(candidates[0]["lead_id"], rival);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_report_carries_counts_precision_agents_and_suggestions(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let beta = profile(&pool, "beta").await;
    for index in 0..10 {
        let confidence = 70.0 + f64::from(index);
        let validated = lead(&pool, &alpha, &format!("v{index}.com"), "validated").await;
        label(
            &pool,
            &validated,
            &alpha,
            "lead_validated",
            confidence,
            "fiable",
            1,
        )
        .await;
        let rejected = lead(&pool, &alpha, &format!("r{index}.com"), "rejected").await;
        label(
            &pool,
            &rejected,
            &alpha,
            "lead_rejected",
            confidence - 50.0,
            "a_verifier",
            1,
        )
        .await;
    }
    let draft = lead(&pool, &alpha, "draft.com", "validated").await;
    label(&pool, &draft, &alpha, "draft_approved", 90.0, "fiable", 1).await;
    let elsewhere = lead(&pool, &beta, "beta.com", "rejected").await;
    label(&pool, &elsewhere, &beta, "lead_rejected", 10.0, "ecarte", 1).await;

    let api = Api::new(pool).await;
    let (status, body) = api.get("/api/v1/quality").await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["decisions"]["validated"], 10);
    assert_eq!(data["decisions"]["rejected"], 11);
    assert_eq!(data["decisions"]["drafts_approved"], 1);
    assert_eq!(data["verdicts"][0]["verdict"], "fiable");
    assert_eq!(data["verdicts"][0]["n"], 10);
    assert_eq!(data["verdicts"][0]["precision"], 1.0);
    assert_eq!(data["verdicts"][2]["verdict"], "ecarte");
    assert_eq!(data["verdicts"][2]["precision"], 0.0);
    assert_eq!(data["profiles"].as_array().unwrap().len(), 2);

    let (_, scoped) = api
        .get(&format!("/api/v1/quality?profile_id={alpha}"))
        .await;
    let data = &scoped["data"];
    assert_eq!(data["decisions"]["rejected"], 10);
    let profiles = data["profiles"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0]["name"], "alpha");
    assert_eq!(profiles[0]["suggestion"]["ready"], true);
    assert_eq!(profiles[0]["suggestion"]["labelled"], 20);
    assert_eq!(profiles[0]["suggestion"]["reliable_from"], 70);
    assert_eq!(profiles[0]["suggestion"]["discard_below"], 65);
    assert_eq!(profiles[0]["current"]["reliable_from"], 70);
    let agent = &data["agents"][0];
    assert_eq!(agent["agent_id"], "fit");
    assert_eq!(agent["validated"], 10);
    assert_eq!(agent["rejected"], 10);
    assert!((agent["separation"].as_f64().unwrap() - 0.5).abs() < 1e-9);

    let (_, beta_only) = api.get(&format!("/api/v1/quality?profile_id={beta}")).await;
    let suggestion = &beta_only["data"]["profiles"][0]["suggestion"];
    assert_eq!(suggestion["ready"], false);
    assert_eq!(suggestion["labelled"], 1);
    assert_eq!(suggestion["required"], 20);
}

#[sqlx::test(migrations = "../../migrations")]
async fn precision_is_split_by_version_and_rejections_by_reason(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    for (index, (kind, model, prompt, reason)) in [
        ("lead_validated", "qwen3", "qualification-agent@a", None),
        ("lead_validated", "qwen3", "qualification-agent@a", None),
        (
            "lead_rejected",
            "qwen3",
            "qualification-agent@a",
            Some("hors_cible"),
        ),
        (
            "lead_rejected",
            "mistral",
            "qualification-agent@b",
            Some("hors_cible"),
        ),
        (
            "lead_rejected",
            "mistral",
            "qualification-agent@b",
            Some("autre"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let status = if kind == "lead_validated" {
            "validated"
        } else {
            "rejected"
        };
        let id = lead(&pool, &alpha, &format!("l{index}.com"), status).await;
        versioned_label(&pool, &id, &alpha, kind, model, prompt, reason).await;
    }

    let api = Api::new(pool).await;
    let (status, body) = api.get("/api/v1/quality").await;

    assert_eq!(status, StatusCode::OK);
    let versions = body["data"]["versions"].as_array().unwrap();
    let rows: Vec<(Value, &str, &str, u64)> = versions
        .iter()
        .map(|row| {
            (
                row["agent_id"].clone(),
                row["model"].as_str().unwrap(),
                row["prompt_version"].as_str().unwrap(),
                row["n"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (Value::Null, "big", "aggregator@x", 5),
            (json!("fit"), "mistral", "qualification-agent@b", 2),
            (json!("fit"), "qwen3", "qualification-agent@a", 3),
        ]
    );
    assert!(versions.iter().all(|row| row["escalated"] == false));
    assert_eq!(versions[0]["agent_name"], Value::Null);
    assert_eq!(versions[0]["kind"], "aggregator");
    assert_eq!(versions[1]["kind"], "qualification");
    assert_eq!(versions[1]["agent_name"], "fit");
    assert_eq!(versions[1]["precision"], 0.0);
    assert!((versions[2]["precision"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);
    assert_eq!(body["data"]["agents"][0]["name"], "fit");
    assert_eq!(
        body["data"]["profiles"][0]["rejection_reasons"],
        json!([{"reason": "hors_cible", "count": 2}, {"reason": "autre", "count": 1}])
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_period_bounds_the_decisions(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let recent = lead(&pool, &alpha, "recent.com", "validated").await;
    label(&pool, &recent, &alpha, "lead_validated", 80.0, "fiable", 2).await;
    let old = lead(&pool, &alpha, "old.com", "rejected").await;
    label(&pool, &old, &alpha, "lead_rejected", 30.0, "a_verifier", 60).await;

    let api = Api::new(pool).await;
    let from = (chrono::Utc::now() - chrono::Duration::days(7))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let (status, body) = api.get(&format!("/api/v1/quality?from={from}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["decisions"]["validated"], 1);
    assert_eq!(body["data"]["decisions"]["rejected"], 0);

    let to = (chrono::Utc::now() - chrono::Duration::days(30))
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let (_, before) = api.get(&format!("/api/v1/quality?to={to}")).await;
    assert_eq!(before["data"]["decisions"]["validated"], 0);
    assert_eq!(before["data"]["decisions"]["rejected"], 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_parameters_and_unknown_profiles_are_refused(pool: PgPool) {
    let api = Api::new(pool).await;
    for uri in [
        "/api/v1/quality?from=yesterday",
        "/api/v1/quality?to=2026-13-40",
        "/api/v1/quality?profile_id=nope",
        "/api/v1/quality?from=2026-10-02T00:00:00Z&to=2026-10-01T00:00:00Z",
    ] {
        let (status, _) = api.get(uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
    let unknown = common::generate_uuid_v7();
    let (status, _) = api
        .get(&format!("/api/v1/quality?profile_id={unknown}"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_report_is_for_authenticated_users_only(pool: PgPool) {
    let harness = Harness::new(pool).await;
    let (anonymous, _, _) =
        send(harness.public(), Method::GET, "/api/v1/quality", None, None).await;
    assert_eq!(anonymous, StatusCode::UNAUTHORIZED);
    let client = format!("Bearer {}", harness.valid_client_token());
    let (service, _, _) = send(
        harness.public(),
        Method::GET,
        "/api/v1/quality",
        Some(&client),
        None,
    )
    .await;
    assert_eq!(service, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_status_change_through_the_api_records_a_label(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let id = lead(&pool, &alpha, "a.com", "qualified").await;
    let api = Api::new(pool).await;
    let (status, _, _) = send(
        api.harness.public(),
        Method::PATCH,
        &format!("/api/v1/leads/{id}/status"),
        Some(&api.bearer),
        Some(json!({"status": "validated"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = api.get("/api/v1/quality").await;
    assert_eq!(body["data"]["decisions"]["validated"], 1);
    assert_eq!(body["data"]["verdicts"][3]["verdict"], Value::Null);
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejecting_a_lead_with_a_reason_stores_it_on_the_label(pool: PgPool) {
    let alpha = profile(&pool, "alpha").await;
    let lead = lead(&pool, &alpha, "nope.com", "qualified").await;
    let api = Api::new(pool.clone()).await;

    let (status, body, _) = send(
        api.harness.public(),
        Method::PATCH,
        &format!("/api/v1/leads/{lead}/status"),
        Some(&api.bearer),
        Some(json!({"status": "rejected", "reason": "autre", "note": "  déjà en discussion  "})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["status"], "rejected");
    let label: (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT kind, rejection_reason, rejection_note FROM decision_labels")
            .fetch_one(&pool)
            .await
            .expect("label");
    assert_eq!(
        label,
        (
            "lead_rejected".to_owned(),
            Some("autre".to_owned()),
            Some("déjà en discussion".to_owned())
        )
    );
}

#[test]
fn openapi_documents_the_quality_path() {
    let spec = serde_json::to_value(api::openapi::ApiDoc::openapi()).expect("spec");
    assert!(spec["paths"]["/api/v1/quality"]["get"].is_object());
}
