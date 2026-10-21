use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    Clock, CreateProfileCommand, FetchError, FetchRequest, FetchResponse, Fetcher, LeadRepository,
    LodestoneUseCase, NormalizedDomain, Observation, ProfileId, ProfileName, Provenance,
    SignalKind, SignalRepository, SourceName, SuppressionTarget, UnitInterval, ValidatedProfile,
    infrastructure::{
        contact_target::postgres::PgAtsTargets, lead::postgres::PgLeadRepository,
        postgres::with_tx, signal::postgres::PgSignalRepository,
    },
};
use serde_json::json;
use sources::ats::{AtsSettings, AtsSource};
use sqlx::PgPool;
use url::Url;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const LEVER_API: &str = "https://api.lever.co/v0/postings/keycloakco?mode=json";
const LEVER_JOBS: &str = r#"[
    {"id":"kc-1","text":"Ingenieur IAM","hostedUrl":"https://jobs.lever.co/keycloakco/kc-1",
     "descriptionPlain":"Operate our self-hosted Keycloak and lead the IAM roadmap.",
     "categories":{"location":"Paris","team":"Platform"},"createdAt":1790000000000},
    {"id":"kc-2","text":"Office manager","hostedUrl":"https://jobs.lever.co/keycloakco/kc-2",
     "descriptionPlain":"Keep the kitchen tidy.","categories":{"location":"Paris","team":"Ops"},"createdAt":1790000000000}
]"#;

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(self.0)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn domain(host: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(host).unwrap()
}

struct SiteFetcher {
    pages: HashMap<String, (&'static str, String)>,
    requests: Mutex<Vec<String>>,
}

impl SiteFetcher {
    fn new(pages: Vec<(&str, &'static str, &str)>) -> Arc<Self> {
        Arc::new(Self {
            pages: pages
                .into_iter()
                .map(|(url, kind, body)| (url.to_owned(), (kind, body.to_owned())))
                .collect(),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requested(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetcher for SiteFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests.lock().unwrap().push(key.clone());
        let (status, content_type, body) = match self.pages.get(&key) {
            Some((kind, body)) => (200, *kind, body.clone().into_bytes()),
            None => (404, "text/html", Vec::new()),
        };
        Ok(FetchResponse {
            status,
            headers: vec![("content-type".to_owned(), content_type.to_owned())],
            body,
            final_url: Url::parse(&key).unwrap(),
            truncated: false,
        })
    }
}

fn site() -> Arc<SiteFetcher> {
    SiteFetcher::new(vec![
        (
            "https://keycloakco.fr/",
            "text/html",
            r#"<a href="https://jobs.lever.co/keycloakco">Jobs</a>"#,
        ),
        (LEVER_API, "application/json", LEVER_JOBS),
        ("https://nojobs.fr/", "text/html", "<p>no careers here</p>"),
        (
            "https://tender.fr/",
            "text/html",
            r#"<a href="https://jobs.lever.co/keycloakco">Jobs</a>"#,
        ),
        (
            "https://optout.io/",
            "text/html",
            r#"<a href="https://jobs.lever.co/keycloakco">Jobs</a>"#,
        ),
    ])
}

fn ats_settings() -> AtsSettings {
    AtsSettings {
        batch_size: 20,
        revisit_interval: Duration::from_secs(7 * 86_400),
        max_career_paths: 2,
        max_rate_limit_wait: Duration::from_secs(1),
    }
}

async fn iam_profile(pool: &PgPool) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
            definition: ValidatedProfile::parse(serde_json::from_str(IAM).unwrap()).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn observe(pool: &PgPool, source: SourceName, host: &str, kind: &str, value: &str) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let active = usecase.list_active_profiles().await.unwrap();
    usecase
        .record_observations(
            &active[0],
            source,
            &[Observation {
                domain: domain(host),
                kind: SignalKind::parse(kind).unwrap(),
                value: Some(value.to_owned()),
                raw: json!({}),
                provenance: Provenance::parse(&format!("https://{host}/probe")).unwrap(),
                strength: UnitInterval::new(0.1).unwrap(),
                observed_at: at(0),
                agent: None,
            }],
        )
        .await
        .unwrap();
}

async fn lead_score(pool: &PgPool, profile: ProfileId, host: &str) -> f64 {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await
    })
    .await
    .unwrap()
    .unwrap()
    .score
    .get()
}

async fn signals(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await?
        else {
            return Ok(Vec::new());
        };
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn run(
    pool: &PgPool,
    fetcher: &Arc<SiteFetcher>,
    clock: &FixedClock,
) -> lodestone_core::triage::TriageReport {
    let source = AtsSource::new(
        Arc::clone(fetcher),
        PgAtsTargets::new(pool.clone()),
        ats_settings(),
    );
    LodestoneUseCase::new(pool.clone())
        .run_triage(&source, clock)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_accepted_job_offer_raises_the_score_of_a_passively_detected_lead(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    observe(
        &pool,
        SourceName::HttpProbe,
        "keycloakco.fr",
        "product_detected",
        "keycloak",
    )
    .await;
    observe(
        &pool,
        SourceName::HttpProbe,
        "nojobs.fr",
        "product_detected",
        "keycloak",
    )
    .await;
    let before = lead_score(&pool, profile, "keycloakco.fr").await;
    assert_eq!(before, lead_score(&pool, profile, "nojobs.fr").await);
    assert!(before > 0.0);

    let fetcher = site();
    let report = run(&pool, &fetcher, &FixedClock(100)).await;

    assert_eq!(report.candidates, 2);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    let offers: Vec<_> = signals(&pool, profile, "keycloakco.fr")
        .await
        .into_iter()
        .filter(|signal| signal.kind.as_str() == "job_offer")
        .collect();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].source, SourceName::Ats);
    assert_eq!(offers[0].agent, None);
    assert_eq!(offers[0].raw["route"], "keywords");
    assert_eq!(offers[0].raw["details"]["ats"], "lever");
    assert_eq!(offers[0].raw["details"]["board"], "keycloakco");
    assert_eq!(offers[0].raw["details"]["job_id"], "kc-1");
    assert_eq!(offers[0].raw["details"]["team"], "Platform");
    assert_eq!(
        offers[0].provenance.as_str(),
        "https://jobs.lever.co/keycloakco/kc-1"
    );
    let after = lead_score(&pool, profile, "keycloakco.fr").await;
    assert!(after > before, "{after} should exceed {before}");
    assert_eq!(lead_score(&pool, profile, "nojobs.fr").await, before);
    assert!(
        signals(&pool, profile, "nojobs.fr")
            .await
            .iter()
            .all(|signal| signal.kind.as_str() != "job_offer")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn every_unsuppressed_lead_is_scanned_whatever_its_source(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    observe(
        &pool,
        SourceName::CertificateTransparency,
        "keycloakco.fr",
        "saas_provider",
        "x",
    )
    .await;
    observe(
        &pool,
        SourceName::Boamp,
        "tender.fr",
        "public_tender",
        "Marche SSO",
    )
    .await;
    observe(
        &pool,
        SourceName::DnsCname,
        "optout.io",
        "saas_provider",
        "auth0",
    )
    .await;
    LodestoneUseCase::new(pool.clone())
        .add_suppression(SuppressionTarget::Domain(domain("optout.io")), None, None)
        .await
        .unwrap();

    let fetcher = site();
    run(&pool, &fetcher, &FixedClock(100)).await;

    let requested = fetcher.requested();
    assert!(
        requested
            .iter()
            .any(|url| url.starts_with("https://keycloakco.fr/"))
    );
    assert!(
        requested
            .iter()
            .any(|url| url.starts_with("https://tender.fr/"))
    );
    assert!(requested.iter().all(|url| !url.contains("optout.io")));
    assert!(
        signals(&pool, profile, "tender.fr")
            .await
            .iter()
            .any(|signal| signal.kind.as_str() == "job_offer")
    );
    assert!(
        signals(&pool, profile, "optout.io")
            .await
            .iter()
            .all(|signal| signal.kind.as_str() != "job_offer")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_rerun_is_idempotent_and_waits_for_the_revisit_interval(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    observe(
        &pool,
        SourceName::HttpProbe,
        "keycloakco.fr",
        "product_detected",
        "keycloak",
    )
    .await;
    observe(
        &pool,
        SourceName::HttpProbe,
        "nojobs.fr",
        "product_detected",
        "keycloak",
    )
    .await;

    let fetcher = site();
    run(&pool, &fetcher, &FixedClock(100)).await;
    let first_requests = fetcher.requested().len();
    let first_score = lead_score(&pool, profile, "keycloakco.fr").await;

    let immediate = run(&pool, &fetcher, &FixedClock(200)).await;
    assert_eq!(immediate.candidates, 0);
    assert_eq!(fetcher.requested().len(), first_requests);

    let later = run(&pool, &fetcher, &FixedClock(8 * 86_400)).await;
    assert_eq!(later.candidates, 2);
    assert!(fetcher.requested().len() > first_requests);
    let offers = signals(&pool, profile, "keycloakco.fr")
        .await
        .into_iter()
        .filter(|signal| signal.kind.as_str() == "job_offer")
        .count();
    assert_eq!(offers, 1);
    assert!(lead_score(&pool, profile, "keycloakco.fr").await <= first_score);
}
