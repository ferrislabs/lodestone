mod support;

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use api::jobs::{Intervals, JobRouter, TrialRuntime, WiredSources};
use axum::Router;
use chrono::{DateTime, TimeZone, Utc};
use common::{EncryptionKey, Secret};
use fetch::{FetchSettings, PoliteFetcher};
use http::{Method, StatusCode};
use lodestone_core::{
    JobId, JobQueue, LeadRepository, NormalizedDomain,
    infrastructure::{
        contact_target::postgres::PgAtsTargets, crypto::AesGcmCipher, job::postgres::PgJobQueue,
        lead::postgres::PgLeadRepository, llm_cache::postgres::PgLlmCache, postgres::with_tx,
    },
    jobs::{JobHandler, JobOutcome},
    trial::PROFILE_TRIAL_JOB_KIND,
};
use serde_json::{Value, json};
use sources::{
    ats::{AtsSettings, AtsSource},
    boamp::{BoampSettings, BoampSource},
    decp::{DecpSettings, DecpSource},
    france_travail::{FranceTravailSettings, FranceTravailSource},
    hacker_news::{HackerNewsSettings, HackerNewsSource},
    ted::{TedSettings, TedSource},
    welcome_to_the_jungle::{
        AccessSource, Endpoints, WelcomeToTheJungleSettings, WelcomeToTheJungleSource,
    },
};
use sqlx::PgPool;
use support::{Harness, send, send_raw};
use url::Url;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

fn iam() -> Value {
    serde_json::from_str(IAM).expect("iam example")
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

struct Api {
    harness: Harness,
    pool: PgPool,
    bearer: String,
}

impl Api {
    async fn new(pool: PgPool) -> Self {
        let harness = Harness::new(pool.clone()).await;
        let bearer = format!("Bearer {}", harness.valid_user_token());
        Self {
            harness,
            pool,
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

    async fn profile(&self) -> String {
        let (status, body) = self
            .call(
                Method::POST,
                "/api/v1/profiles",
                Some(json!({ "name": "iam", "definition": iam() })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["id"].as_str().expect("id").to_owned()
    }

    async fn decided(&self, profile: &str, host: &str, kind: &str, seconds: i64) -> Uuid {
        let profile_id = profile.parse().expect("profile id");
        let lead = with_tx(&self.pool, async |shared| {
            PgLeadRepository::new(&shared)
                .record_seen(
                    profile_id,
                    &NormalizedDomain::from_host(host).expect("domain"),
                    at(0),
                )
                .await
        })
        .await
        .expect("lead")
        .id;
        sqlx::query(
            "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
             VALUES ($1, $2, $3, 1, $4, $5, $6)",
        )
        .bind(Uuid::now_v7())
        .bind(lead.0)
        .bind(profile_id.0)
        .bind(kind)
        .bind(at(seconds))
        .bind(json!({
            "confidence": null, "opinion": null, "evidence": null,
            "verdict": null, "verdict_reason": null, "agents": {}
        }))
        .execute(&self.pool)
        .await
        .expect("label");
        lead.0
    }

    async fn ready(&self) -> String {
        let profile = self.profile().await;
        self.decided(&profile, "alpha.com", "lead_validated", 100)
            .await;
        self.decided(&profile, "bravo.com", "lead_rejected", 200)
            .await;
        profile
    }

    async fn trial(&self, profile: &str, body: Value) -> (StatusCode, Value) {
        self.call(
            Method::POST,
            &format!("/api/v1/profiles/{profile}/trials"),
            Some(body),
        )
        .await
    }
}

fn draft() -> Value {
    let mut draft = iam();
    draft["identity"]["offer"] = json!("Managed IAM draft");
    draft
}

#[sqlx::test(migrations = "../../migrations")]
async fn creating_a_trial_answers_202_with_the_defaults_and_queues_its_job(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let profile = api.ready().await;

    let (status, body) = api.trial(&profile, json!({ "definition": draft() })).await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    let trial = &body["data"];
    assert!(trial["id"].is_string());
    assert_eq!(trial["profile_id"], profile.as_str());
    assert_eq!(trial["base_revision"], 1);
    assert_eq!(trial["status"], "pending");
    assert_eq!(trial["sample_size"], 20);
    assert_eq!(trial["budget_cents"], 200);
    assert_eq!(trial["spent_cents"], 0.0);
    assert_eq!(trial["leads_tried"], 0);
    assert_eq!(trial["error"], Value::Null);
    assert!(trial["created_at"].is_string());
    assert_eq!(trial["started_at"], Value::Null);
    assert_eq!(trial["finished_at"], Value::Null);
    let jobs: Vec<(Value, Option<String>)> =
        sqlx::query_as("SELECT payload, dedup_key FROM jobs WHERE kind = $1")
            .bind(PROFILE_TRIAL_JOB_KIND)
            .fetch_all(&pool)
            .await
            .unwrap();
    let id = trial["id"].as_str().unwrap();
    assert_eq!(
        jobs,
        [(json!({ "trial_id": id }), Some(format!("trial:{id}")))]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_limits_of_a_trial_are_the_ones_the_caller_asks_for(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.ready().await;

    let (status, body) = api
        .trial(
            &profile,
            json!({ "definition": draft(), "sample_size": 50, "budget_cents": 2000 }),
        )
        .await;

    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_eq!(body["data"]["sample_size"], 50);
    assert_eq!(body["data"]["budget_cents"], 2000);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_invalid_draft_is_a_422_with_the_faulty_path_like_a_revision(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.ready().await;
    let mut bad_version = iam();
    bad_version["schema_version"] = json!(99);
    let mut unknown_field = iam();
    unknown_field["identity"]["colour"] = json!("red");
    let mut removed_weights = iam();
    removed_weights["scoring"] = json!({"factors": []});

    for (definition, path) in [
        (bad_version, "schema_version"),
        (unknown_field, "identity.colour"),
        (removed_weights, "scoring"),
    ] {
        let (status, answer) = api
            .trial(&profile, json!({ "definition": definition }))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{answer}");
        assert_eq!(answer["code"], "E_UNPROCESSABLE_ENTITY");
        assert_eq!(answer["details"][0]["path"], path, "{answer}");
        assert!(answer["details"][0]["message"].is_string());
    }
    let (_, listing) = api
        .call(
            Method::GET,
            &format!("/api/v1/profiles/{profile}/trials"),
            None,
        )
        .await;
    assert_eq!(listing["data"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_bad_limit_or_body_is_a_400(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.ready().await;

    let cases = [
        (
            json!({ "definition": draft(), "sample_size": 0 }),
            "sample_size",
        ),
        (
            json!({ "definition": draft(), "sample_size": 51 }),
            "sample_size",
        ),
        (
            json!({ "definition": draft(), "budget_cents": 0 }),
            "budget_cents",
        ),
        (
            json!({ "definition": draft(), "budget_cents": 2001 }),
            "budget_cents",
        ),
        (json!({ "sample_size": 5 }), "malformed request"),
        (
            json!({ "definition": draft(), "sample_size": -1 }),
            "malformed request",
        ),
    ];
    for (body, needle) in cases {
        let (status, answer) = api.trial(&profile, body.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body} {answer}");
        assert_eq!(answer["code"], "E_BAD_REQUEST");
        assert!(
            answer["message"].as_str().unwrap().contains(needle),
            "{answer}"
        );
    }
    let (status, _, raw) = send_raw(
        api.app(),
        Method::POST,
        &format!("/api/v1/profiles/{profile}/trials"),
        Some(&api.bearer),
        Some("{not json".to_owned()),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{raw}");
    let (status, _) = api
        .call(
            Method::POST,
            "/api/v1/profiles/not-a-uuid/trials",
            Some(json!({ "definition": draft() })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, listing) = api
        .call(
            Method::GET,
            &format!("/api/v1/profiles/{profile}/trials"),
            None,
        )
        .await;
    assert_eq!(listing["data"], json!([]));
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_trial_of_an_unknown_profile_is_a_404(pool: PgPool) {
    let api = Api::new(pool).await;

    let (status, body) = api
        .trial(
            &Uuid::now_v7().to_string(),
            json!({ "definition": draft() }),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, _) = api
        .call(
            Method::GET,
            &format!("/api/v1/profiles/{}/trials", Uuid::now_v7()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = api
        .call(
            Method::GET,
            &format!("/api/v1/trials/{}", Uuid::now_v7()),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = api
        .call(Method::GET, "/api/v1/trials/not-a-uuid", None)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_live_trial_and_a_profile_without_decided_lead_are_409s(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let profile = api.ready().await;
    let empty = api.profile_named("empty").await;

    let (status, first) = api.trial(&profile, json!({ "definition": draft() })).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{first}");
    let (status, second) = api.trial(&profile, json!({ "definition": draft() })).await;
    let (none, answer) = api.trial(&empty, json!({ "definition": draft() })).await;

    assert_eq!(status, StatusCode::CONFLICT, "{second}");
    assert_eq!(second["code"], "E_CONFLICT");
    assert_eq!(none, StatusCode::CONFLICT, "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains("no decided lead to try")
    );
    sqlx::query("UPDATE profile_trials SET status = 'done', finished_at = $1")
        .bind(at(10))
        .execute(&pool)
        .await
        .unwrap();
    let (status, third) = api.trial(&profile, json!({ "definition": draft() })).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{third}");
}

impl Api {
    async fn profile_named(&self, name: &str) -> String {
        let mut definition = iam();
        definition["identity"]["offer"] = json!(format!("Offer of {name}"));
        let (status, body) = self
            .call(
                Method::POST,
                "/api/v1/profiles",
                Some(json!({ "name": name, "definition": definition })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["data"]["id"].as_str().expect("id").to_owned()
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_users_may_use_trials_and_the_token_is_required(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.ready().await;
    let (_, created) = api.trial(&profile, json!({ "definition": draft() })).await;
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    let client = format!("Bearer {}", api.harness.valid_client_token());
    let requests = [
        (
            Method::POST,
            format!("/api/v1/profiles/{profile}/trials"),
            Some(json!({ "definition": draft() })),
        ),
        (
            Method::GET,
            format!("/api/v1/profiles/{profile}/trials"),
            None,
        ),
        (Method::GET, format!("/api/v1/trials/{id}"), None),
    ];

    for (method, uri, body) in requests {
        let (status, _, _) =
            send(api.app(), method.clone(), &uri, Some(&client), body.clone()).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
        let (status, _, _) = send(api.app(), method.clone(), &uri, None, body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
    }
}

async fn seed_report(
    pool: &PgPool,
    trial: &str,
    leads: &[(Uuid, &str, Option<&str>, Option<&str>)],
) {
    let trial: Uuid = trial.parse().unwrap();
    for (position, (lead, label, current, tried)) in leads.iter().enumerate() {
        sqlx::query(
            "INSERT INTO profile_trial_leads (trial_id, lead_id, label, current_verdict, current_confidence, trial_verdict, trial_confidence, error, tried_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(trial)
        .bind(lead)
        .bind(label)
        .bind(current)
        .bind(current.map(|_| 80.0))
        .bind(tried)
        .bind(tried.map(|_| 65.5))
        .bind(tried.is_none().then_some("Échec de l'appel au modèle"))
        .bind(at(300 + i64::try_from(position).unwrap()))
        .execute(pool)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE profile_trials SET status = 'done', spent_cents = 12.5, started_at = $2, finished_at = $3 WHERE id = $1")
        .bind(trial)
        .bind(at(290))
        .bind(at(310))
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_detail_compares_the_verdicts_with_the_labels(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let profile = api.profile().await;
    let a = api
        .decided(&profile, "alpha.com", "lead_validated", 100)
        .await;
    let b = api
        .decided(&profile, "bravo.com", "lead_rejected", 200)
        .await;
    let c = api
        .decided(&profile, "charlie.com", "lead_rejected", 201)
        .await;
    let d = api
        .decided(&profile, "delta.com", "lead_validated", 202)
        .await;
    let (_, created) = api.trial(&profile, json!({ "definition": draft() })).await;
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    seed_report(
        &pool,
        &id,
        &[
            (a, "validated", Some("fiable"), Some("fiable")),
            (b, "rejected", Some("fiable"), Some("ecarte")),
            (c, "rejected", Some("a_verifier"), Some("a_verifier")),
            (d, "validated", Some("fiable"), None),
        ],
    )
    .await;

    let (status, body) = api
        .call(Method::GET, &format!("/api/v1/trials/{id}"), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["id"], id.as_str());
    assert_eq!(data["status"], "done");
    assert_eq!(data["leads_tried"], 4);
    assert_eq!(data["spent_cents"], 12.5);
    assert_eq!(data["started_at"], at(290).to_rfc3339().as_str());
    assert_eq!(data["finished_at"], at(310).to_rfc3339().as_str());
    assert_eq!(
        data["summary"],
        json!({
            "current": {
                "reliable": 2,
                "reliable_validated": 1,
                "precision": 0.5,
                "rejected": 2,
                "rejected_discarded": 0
            },
            "trial": {
                "reliable": 1,
                "reliable_validated": 1,
                "precision": 1.0,
                "rejected": 2,
                "rejected_discarded": 1
            },
            "changed": 1,
            "errors": 1
        })
    );
    let leads = data["leads"].as_array().unwrap();
    assert_eq!(leads.len(), 4);
    let first = &leads[0];
    assert_eq!(first["lead_id"], a.to_string().as_str());
    assert_eq!(first["domain"], "alpha.com");
    assert_eq!(first["label"], "validated");
    assert_eq!(first["current_verdict"], "fiable");
    assert_eq!(first["current_confidence"], 80.0);
    assert_eq!(first["trial_verdict"], "fiable");
    assert_eq!(first["trial_confidence"], 65.5);
    assert_eq!(first["error"], Value::Null);
    assert_eq!(leads[1]["trial_verdict"], "ecarte");
    assert_eq!(leads[2]["trial_verdict"], "a_verifier");
    assert_eq!(leads[3]["trial_verdict"], Value::Null);
    assert_eq!(leads[3]["error"], "Échec de l'appel au modèle");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_list_is_newest_first_and_capped_at_twenty(pool: PgPool) {
    let api = Api::new(pool.clone()).await;
    let profile = api.ready().await;
    let profile_id: Uuid = profile.parse().unwrap();
    let definition = stored_draft();
    for position in 0..22_i64 {
        sqlx::query(
            "INSERT INTO profile_trials (id, profile_id, base_revision, definition, status, sample_size, budget_cents, created_at, finished_at)
             VALUES ($1, $2, 1, $3, 'done', 20, 200, $4, $4)",
        )
        .bind(Uuid::now_v7())
        .bind(profile_id)
        .bind(&definition)
        .bind(at(1_000 + position))
        .execute(&pool)
        .await
        .unwrap();
    }

    let (status, body) = api
        .call(
            Method::GET,
            &format!("/api/v1/profiles/{profile}/trials"),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let listed = body["data"].as_array().unwrap();
    assert_eq!(listed.len(), 20);
    let dates: Vec<&str> = listed
        .iter()
        .map(|trial| trial["created_at"].as_str().unwrap())
        .collect();
    let mut sorted = dates.clone();
    sorted.sort_by(|left, right| right.cmp(left));
    assert_eq!(dates, sorted);
    assert_eq!(dates[0], at(1_021).to_rfc3339());
    assert_eq!(dates[19], at(1_002).to_rfc3339());
    assert_eq!(listed[0]["leads_tried"], 0);
}

fn stored_draft() -> Value {
    lodestone_core::ValidatedProfile::parse(draft())
        .expect("valid draft")
        .to_json()
}

fn polite() -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: std::num::NonZeroU32::new(10_000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    )
}

fn router(api: &Api, runtime: Option<TrialRuntime>) -> JobRouter {
    let fetcher = polite();
    let url = |path: &str| Url::parse(&format!("http://127.0.0.1:1{path}")).unwrap();
    let router = JobRouter::new(
        api.harness.state.usecase.clone(),
        WiredSources {
            boamp: BoampSource::new(
                Arc::clone(&fetcher),
                BoampSettings {
                    base_url: url("/boamp").into(),
                    lookback_days: 7,
                    max_pages: 1,
                },
            ),
            decp: DecpSource::new(
                Arc::clone(&fetcher),
                DecpSettings {
                    dataset_api_url: url("/decp").into(),
                    annuaire_url: url("/annuaire"),
                    entreprises_url: url("/company").into(),
                    lookback_days: 7,
                    notified_within_days: 7,
                    max_body_bytes: 1024 * 1024,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
            ted: TedSource::new(
                Arc::clone(&fetcher),
                TedSettings {
                    base_url: url("/ted").into(),
                    lookback_days: 7,
                    max_pages: 1,
                    buyer_countries: Vec::new(),
                },
            ),
            france_travail: FranceTravailSource::new(
                Arc::clone(&fetcher),
                api::providers::CatalogAccess::new(
                    api.harness.state.usecase.clone(),
                    Arc::clone(&api.harness.state.providers),
                    Arc::clone(&api.harness.state.cipher),
                ),
                FranceTravailSettings {
                    published_since_days: 7,
                    max_pages: 1,
                },
            ),
            hacker_news: HackerNewsSource::new(
                Arc::clone(&fetcher),
                HackerNewsSettings {
                    base_url: url("/hn/").into(),
                    max_pages: 1,
                },
            ),
            welcome_to_the_jungle: WelcomeToTheJungleSource::new(
                Arc::clone(&fetcher),
                WelcomeToTheJungleSettings {
                    endpoints: Endpoints {
                        site: url("/wttj/"),
                        search: None,
                    },
                    access: AccessSource::Catalog(api.harness.state.usecase.clone()),
                    max_company_lookups: 1,
                },
            ),
            ats: AtsSource::new(
                fetcher,
                PgAtsTargets::new(api.pool.clone()),
                AtsSettings {
                    batch_size: 1,
                    revisit_interval: Duration::from_secs(60),
                    max_career_paths: 1,
                    max_rate_limit_wait: Duration::from_secs(1),
                },
            ),
        },
        Intervals {
            other_sources: Duration::from_secs(6 * 3600),
            decp: Duration::from_secs(6 * 3600),
            hacker_news: Duration::from_secs(24 * 3600),
        },
    );
    match runtime {
        Some(runtime) => router.with_trials(runtime),
        None => router,
    }
}

fn runtime(pool: &PgPool) -> TrialRuntime {
    TrialRuntime {
        cipher: AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(support::KEY)).unwrap())
            .unwrap(),
        cache: PgLlmCache::new(pool.clone()),
        concurrency: NonZeroUsize::MIN,
    }
}

async fn queued(api: &Api, id: &str) -> lodestone_core::Job {
    let job: Uuid = sqlx::query_scalar("SELECT id FROM jobs WHERE dedup_key = $1")
        .bind(format!("trial:{id}"))
        .fetch_one(&api.pool)
        .await
        .unwrap();
    PgJobQueue::new(api.pool.clone())
        .find(JobId(job))
        .await
        .unwrap()
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_router_without_a_model_runtime_fails_the_trial_with_a_reason(pool: PgPool) {
    let api = Api::new(pool).await;
    let profile = api.ready().await;
    let (_, created) = api.trial(&profile, json!({ "definition": draft() })).await;
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    let job = queued(&api, &id).await;

    let outcome = router(&api, None).handle(&job).await.unwrap();

    assert_eq!(outcome, JobOutcome::Done);
    let (_, detail) = api
        .call(Method::GET, &format!("/api/v1/trials/{id}"), None)
        .await;
    assert_eq!(detail["data"]["status"], "failed");
    assert_eq!(detail["data"]["error"], "no LLM provider is configured");
    assert!(detail["data"]["finished_at"].is_string());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_trial_job_runs_the_trial_and_records_a_lead_without_provider_as_an_error(
    pool: PgPool,
) {
    let api = Api::new(pool.clone()).await;
    let profile = api.ready().await;
    let (_, created) = api.trial(&profile, json!({ "definition": draft() })).await;
    let id = created["data"]["id"].as_str().unwrap().to_owned();
    let job = queued(&api, &id).await;

    let outcome = router(&api, Some(runtime(&pool)))
        .handle(&job)
        .await
        .unwrap();

    assert_eq!(outcome, JobOutcome::Done);
    let (_, detail) = api
        .call(Method::GET, &format!("/api/v1/trials/{id}"), None)
        .await;
    assert_eq!(detail["data"]["status"], "done");
    assert_eq!(detail["data"]["leads_tried"], 2);
    assert_eq!(detail["data"]["summary"]["errors"], 2);
    assert!(
        detail["data"]["leads"]
            .as_array()
            .unwrap()
            .iter()
            .all(|lead| lead["trial_verdict"].is_null() && lead["error"].is_string())
    );
    let again = router(&api, Some(runtime(&pool)))
        .handle(&job)
        .await
        .unwrap();
    assert_eq!(again, JobOutcome::Done);
}
