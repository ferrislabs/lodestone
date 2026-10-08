use std::{fs, num::NonZeroU32, path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    Clock, CreateProfileCommand, EcosystemModule, EnrichmentStatus, FetchError, FetchRequest,
    FetchResponse, Fetcher, LeadRepository, LodestoneUseCase, ModuleStatus, NormalizedDomain,
    Observation, ProfileId, ProfileName, Provenance, SignalKind, SignalRepository, SourceName,
    UnitInterval, ValidatedProfile,
    ecosystem::InspectionRunSettings,
    infrastructure::{
        candidate_host::PgCandidateHosts, contact_target::postgres::PgAtsTargets,
        lead::postgres::PgLeadRepository, postgres::with_tx, signal::postgres::PgSignalRepository,
    },
};
use serde_json::{Value, from_str, json};
use sources::{
    ats::{AtsSettings, AtsSource},
    bodacc::{BODACC_URL, BodaccSettings},
    company::CompanySettings,
    decp::holders::{DECP_HOLDERS_URL, HolderSettings},
    ecosystem::{EcosystemConfig, EcosystemModules, WebsiteSettings},
    endpoint::Endpoint,
    http_probe::{HttpProbeSettings, HttpProbeSource, ProbeScheme},
};
use sqlx::{PgPool, query};
use url::Url;

mod fixtures_support;

const TENDER_DOMAIN: &str = "mairie-exemple.fr";
const HOME_PAGE: &str = r#"<html><head><title>Mairie exemple</title><meta name="generator" content="Drupal 10"></head><body><h1>Bienvenue sur le site de la mairie</h1><p>Services municipaux, démarches en ligne et actualités de la commune.</p><a href="https://jobs.lever.co/mairieexemple">Nous rejoindre</a></body></html>"#;
const LEVER_JOBS: &str = r#"[
    {"id":"iam-1","text":"Ingénieur IAM Keycloak","hostedUrl":"https://jobs.lever.co/mairieexemple/iam-1",
     "descriptionPlain":"Exploiter notre Keycloak auto-hébergé et piloter la feuille de route IAM.",
     "categories":{"location":"Lyon","team":"DSI"},"createdAt":1790000000000},
    {"id":"rh-1","text":"Chargé de paie","hostedUrl":"https://jobs.lever.co/mairieexemple/rh-1",
     "descriptionPlain":"Gestion de la paie des agents.","categories":{"location":"Lyon","team":"RH"},"createdAt":1790000000000}
]"#;

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn polite() -> PoliteFetcher {
    PoliteFetcher::new(FetchSettings {
        contact: "bot@example.com".to_owned(),
        requests_per_host_per_minute: NonZeroU32::new(600).unwrap(),
        request_timeout: Duration::from_secs(5),
        max_body_bytes: 2 * 1024 * 1024,
        max_redirects: 3,
        robots_ttl: Duration::from_secs(60),
        source_quotas: Vec::new(),
        allow_private_targets: true,
    })
    .unwrap()
}

struct MockedWeb {
    inner: PoliteFetcher,
    base: String,
}

impl Fetcher for MockedWeb {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let host = request.url.host_str().unwrap_or_default();
        let mut url = Url::parse(&format!("{}/{host}{}", self.base, request.url.path()))
            .map_err(|_| FetchError::Timeout)?;
        url.set_query(request.url.query());
        self.inner
            .fetch(&FetchRequest {
                url,
                purpose: request.purpose,
                source: request.source,
            })
            .await
    }
}

async fn active_iam_profile(pool: &PgPool) -> ProfileId {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/profiles/iam-ferriskey.json");
    let value: Value = from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse("iam").unwrap(),
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn record_tender(pool: &PgPool) {
    let usecase = LodestoneUseCase::new(pool.clone());
    let active = usecase.list_active_profiles().await.unwrap();
    usecase
        .record_observations(
            &active[0],
            SourceName::Boamp,
            &[Observation {
                domain: NormalizedDomain::from_host(TENDER_DOMAIN).unwrap(),
                kind: SignalKind::parse("public_tender").unwrap(),
                value: Some("Mise en place d'un SSO".to_owned()),
                raw: json!({"route": "keywords"}),
                provenance: Provenance::parse("https://www.boamp.fr/avis/detail/26-1").unwrap(),
                strength: UnitInterval::new(0.7).unwrap(),
                observed_at: at(10),
                agent: None,
            }],
        )
        .await
        .unwrap();
}

async fn lead_signals(pool: &PgPool, profile: ProfileId) -> Vec<(String, String)> {
    let domain = NormalizedDomain::from_host(TENDER_DOMAIN).unwrap();
    let mut found: Vec<(String, String)> = with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain)
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
    .into_iter()
    .map(|signal| {
        (
            signal.source.as_str().to_owned(),
            signal.kind.as_str().to_owned(),
        )
    })
    .collect();
    found.sort();
    found.dedup();
    found
}

async fn ready_providers(pool: &PgPool) {
    for key in ["company_research", "bodacc", "decp"] {
        query(
            "INSERT INTO source_provider_configurations (provider_key, settings, status, created_at, updated_at) VALUES ($1, '{}'::jsonb, 'ready', now(), now()) ON CONFLICT (provider_key) DO UPDATE SET status = 'ready'",
        )
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_boamp_lead_is_enriched_by_the_http_probe_and_its_job_offers(pool: PgPool) {
    let profile = active_iam_profile(&pool).await;
    ready_providers(&pool).await;
    record_tender(&pool).await;
    let realm = fixtures_support::load("keycloak/kc26-realms-master.json");
    let server = MockServer::start();
    let probe = server.mock(|when, then| {
        when.method(GET).path("/realms/master");
        then.status(realm.status)
            .header("content-type", "application/json")
            .body(&realm.body);
    });
    let home = server.mock(|when, then| {
        when.method(GET).path(format!("/{TENDER_DOMAIN}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(HOME_PAGE);
    });
    let site_realm = server.mock(|when, then| {
        when.method(GET)
            .path(format!("/{TENDER_DOMAIN}/realms/master"));
        then.status(realm.status)
            .header("content-type", "application/json")
            .body(&realm.body);
    });
    let board = server.mock(|when, then| {
        when.method(GET)
            .path("/api.lever.co/v0/postings/mairieexemple")
            .query_param("mode", "json");
        then.status(200)
            .header("content-type", "application/json")
            .body(LEVER_JOBS);
    });
    let usecase = Arc::new(LodestoneUseCase::new(pool.clone()));

    let http_probe = HttpProbeSource::new(
        Arc::new(polite()),
        Arc::new(PgCandidateHosts::new(pool.clone())),
        Arc::clone(&usecase),
        HttpProbeSettings {
            batch_size: 100,
            concurrency: 4,
            revisit_interval: Duration::from_secs(3600),
            scheme: ProbeScheme::Http,
            port: None,
            host_override: Some(format!("127.0.0.1:{}", server.port())),
        },
    );
    let probed = usecase.run_source(&http_probe, &FixedClock).await.unwrap();
    assert_eq!(probed.items, 7);
    assert_eq!(probe.hits(), 7);

    let company_jobs = AtsSource::new(
        Arc::new(MockedWeb {
            inner: polite(),
            base: server.base_url(),
        }),
        PgAtsTargets::new(pool.clone()),
        AtsSettings {
            batch_size: 20,
            revisit_interval: Duration::from_secs(7 * 86_400),
            max_career_paths: 2,
            max_rate_limit_wait: Duration::from_secs(1),
        },
    );
    let report = usecase
        .run_triage(&company_jobs, &FixedClock)
        .await
        .unwrap();
    assert_eq!(home.hits(), 1);
    assert_eq!(board.hits(), 1);
    assert_eq!(report.candidates, 2);
    assert_eq!(report.accepted_by_rules, 1);

    assert_eq!(
        lead_signals(&pool, profile).await,
        [
            ("ats".to_owned(), "job_offer".to_owned()),
            ("boamp".to_owned(), "public_tender".to_owned()),
            ("http_probe".to_owned(), "product_detected".to_owned()),
        ]
    );
    let lead = with_tx(&pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(
                profile,
                &NormalizedDomain::from_host(TENDER_DOMAIN).unwrap(),
            )
            .await
    })
    .await
    .unwrap()
    .unwrap();
    let before = usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(before.enrichment.status, EnrichmentStatus::TimedOut);
    assert!(
        before
            .enrichment
            .modules
            .iter()
            .all(|state| state.status == ModuleStatus::Pending)
    );

    let modules = EcosystemModules::new(
        Arc::new(MockedWeb {
            inner: polite(),
            base: server.base_url(),
        }),
        EcosystemConfig {
            company: CompanySettings {
                entreprises_url: Endpoint::from(
                    Url::parse(&format!("{}/entreprises", server.base_url())).unwrap(),
                ),
                max_pages: 2,
                max_rate_limit_wait: Duration::from_secs(1),
                origin: None,
            },
            bodacc: BodaccSettings {
                dataset_url: Endpoint::from(Url::parse(BODACC_URL).unwrap()),
                max_rate_limit_wait: Duration::from_secs(1),
            },
            holders: HolderSettings {
                holders_url: Endpoint::from(Url::parse(DECP_HOLDERS_URL).unwrap()),
                max_rate_limit_wait: Duration::from_secs(1),
            },
            website: WebsiteSettings {
                max_rate_limit_wait: Duration::from_secs(1),
                origin: None,
            },
            max_career_paths: 2,
            max_rate_limit_wait: Duration::from_secs(1),
        },
    );
    let inspection = usecase
        .inspect_due_domains(
            &modules.runners(),
            InspectionRunSettings { batch_size: 10 },
            &FixedClock,
        )
        .await
        .unwrap();
    assert_eq!(inspection.domains, 5);
    assert_eq!(inspection.not_applicable, 3);
    assert!(site_realm.hits() >= 1);
    let detail = usecase.lead_detail(lead.id).await.unwrap();
    assert_eq!(detail.enrichment.status, EnrichmentStatus::Complete);
    for module in [
        EcosystemModule::OfficialIdentity,
        EcosystemModule::LegalLife,
        EcosystemModule::PublicContracts,
    ] {
        let state = detail.ecosystem.state_of(module);
        assert_eq!(state.status, ModuleStatus::NotApplicable, "{module:?}");
        assert_eq!(state.error.as_deref(), Some("SIREN inconnu"));
    }
    let website: Vec<(&str, &str)> = detail
        .ecosystem
        .findings_of(EcosystemModule::WebsiteStack)
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert!(website.contains(&("generator", "Drupal 10")));
    assert!(
        website
            .iter()
            .any(|(kind, value)| *kind == "product" && value.starts_with("keycloak"))
    );
    let offers: Vec<(&str, bool)> = detail
        .ecosystem
        .findings_of(EcosystemModule::HiringActivity)
        .filter(|found| found.kind == "job_offer")
        .map(|found| {
            (
                found.value.as_str(),
                found.details["highlighted"].as_bool().unwrap_or(false),
            )
        })
        .collect();
    assert_eq!(
        offers,
        [("Ingénieur IAM Keycloak", true), ("Chargé de paie", false)]
    );
    let again = usecase
        .inspect_due_domains(
            &modules.runners(),
            InspectionRunSettings { batch_size: 10 },
            &FixedClock,
        )
        .await
        .unwrap();
    assert_eq!(again.domains, 0);
    let stats = usecase.ecosystem_stats(at(200)).await.unwrap();
    let hiring = stats
        .iter()
        .find(|stats| stats.module == EcosystemModule::HiringActivity)
        .unwrap();
    assert_eq!(hiring.leads_inspected, 1);
    assert_eq!(hiring.last_run, Some(at(100)));
    assert!(hiring.findings_count >= 3);
    assert!(hiring.latest_example.is_some());
}
