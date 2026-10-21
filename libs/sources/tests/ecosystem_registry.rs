use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    Clock, CreateProfileCommand, DnsError, DnsRecords, DnsResolver, EcosystemModule, FetchError,
    FetchRequest, FetchResponse, Fetcher, Hostname, LeadRepository, LodestoneUseCase, ModuleRunner,
    ModuleStatus, NormalizedDomain, Observation, ProfileId, ProfileName, Provenance, SignalKind,
    SourceName, UnitInterval, ValidatedProfile,
    ecosystem::InspectionRunSettings,
    infrastructure::{lead::postgres::PgLeadRepository, postgres::with_tx},
};
use serde_json::{Value, from_str, json};
use sources::{
    bodacc::BodaccSettings,
    company::CompanySettings,
    decp::holders::HolderSettings,
    ecosystem::{EcosystemConfig, EcosystemModules, WebsiteSettings},
    endpoint::Endpoint,
};
use sqlx::{PgPool, query, query_as};
use url::Url;

const DOMAIN: &str = "acme-logiciels.fr";
const HOME: &str = r#"<html><head><title>Acme logiciels</title></head><body><h1>Acme, logiciels de gestion pour les collectivités</h1><p>Nous accompagnons les communes depuis 2004.</p><a href="/mentions-legales">Mentions légales</a></body></html>"#;
const LEGAL: &str = r#"<html><head><title>Mentions légales</title></head><body><h1>Mentions légales</h1><p>Acme Logiciels SAS, au capital de 10 000 euros, SIREN 552 100 554, RCS Lyon. Hébergement : OVH.</p></body></html>"#;
const ANNOUNCEMENTS: &str = include_str!("fixtures/bodacc/annonces-552100554.json");
const HOLDER: &str = include_str!("fixtures/decp/titulaire-552100554.json");

struct StepClock(AtomicI64);

impl Clock for StepClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
            + ChronoDuration::hours(self.0.load(Ordering::SeqCst))
    }
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

#[derive(Default)]
struct NoRecords;

impl DnsRecords for NoRecords {
    async fn mx_hosts(&self, _domain: &NormalizedDomain) -> Result<Vec<Hostname>, DnsError> {
        Ok(Vec::new())
    }

    async fn txt_records(&self, _name: &str) -> Result<Vec<String>, DnsError> {
        Ok(Vec::new())
    }
}

impl DnsResolver for NoRecords {
    async fn cname_chain(&self, _host: &Hostname) -> Result<Vec<Hostname>, DnsError> {
        Ok(Vec::new())
    }
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
        let mut response = self
            .inner
            .fetch(&FetchRequest {
                url,
                purpose: request.purpose,
                source: request.source,
            })
            .await?;
        response.final_url = request.url.clone();
        Ok(response)
    }
}

fn modules(server: &MockServer) -> EcosystemModules<MockedWeb, NoRecords> {
    let wait = Duration::from_secs(1);
    EcosystemModules::new(
        Arc::new(MockedWeb {
            inner: polite(),
            base: server.base_url(),
        }),
        Arc::new(NoRecords),
        EcosystemConfig {
            company: CompanySettings {
                entreprises_url: Endpoint::from(
                    Url::parse("https://entreprises.test/search").unwrap(),
                ),
                max_pages: 3,
                max_rate_limit_wait: wait,
                origin: None,
            },
            bodacc: BodaccSettings {
                dataset_url: Endpoint::from(
                    Url::parse("https://bodacc.test/datasets/annonces-commerciales").unwrap(),
                ),
                max_rate_limit_wait: wait,
            },
            holders: HolderSettings {
                holders_url: Endpoint::from(Url::parse("https://decp.test/data/").unwrap()),
                max_rate_limit_wait: wait,
            },
            website: WebsiteSettings {
                max_rate_limit_wait: wait,
                origin: None,
            },
            max_career_paths: 1,
            max_rate_limit_wait: wait,
        },
    )
}

async fn active_profile(pool: &PgPool) -> ProfileId {
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
    let active = usecase.list_active_profiles().await.unwrap();
    usecase
        .record_observations(
            &active[0],
            SourceName::Boamp,
            &[Observation {
                domain: NormalizedDomain::from_host(DOMAIN).unwrap(),
                kind: SignalKind::parse("public_tender").unwrap(),
                value: Some("Mise en place d'un SSO".to_owned()),
                raw: json!({"route": "keywords"}),
                provenance: Provenance::parse("https://www.boamp.fr/avis/detail/26-2").unwrap(),
                strength: UnitInterval::new(0.7).unwrap(),
                observed_at: Utc.with_ymd_and_hms(2026, 10, 7, 9, 0, 0).unwrap(),
                agent: None,
            }],
        )
        .await
        .unwrap();
    id
}

struct NextRun {
    status: String,
    next_run_at: DateTime<Utc>,
    error: Option<String>,
}

async fn state_row(pool: &PgPool, module: EcosystemModule) -> NextRun {
    query_as!(
        NextRun,
        "SELECT status, next_run_at, error FROM lead_ecosystem_modules WHERE domain = $1 AND module = $2",
        DOMAIN,
        module.as_str(),
    )
    .fetch_one(pool)
    .await
    .unwrap()
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
async fn the_registry_modules_read_the_siren_found_by_the_identity_module(pool: PgPool) {
    let profile = active_profile(&pool).await;
    ready_providers(&pool).await;
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path(format!("/{DOMAIN}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(HOME);
    });
    server.mock(|when, then| {
        when.method(GET).path(format!("/{DOMAIN}/mentions-legales"));
        then.status(200)
            .header("content-type", "text/html")
            .body(LEGAL);
    });
    let register = server.mock(|when, then| {
        when.method(GET)
            .path("/entreprises.test/search")
            .query_param("q", "552100554");
        then.status(200).json_body(json!({"results": [{
            "siren": "552100554",
            "nom_complet": "ACME LOGICIELS",
            "activite_principale": "62.01Z",
            "section_activite_principale": "J",
            "tranche_effectif_salarie": "12",
            "date_creation": "2004-06-09",
            "siege": {"code_postal": "69002", "libelle_commune": "LYON"}
        }]}));
    });
    let mut failing = server.mock(|when, then| {
        when.method(GET)
            .path("/bodacc.test/datasets/annonces-commerciales/records")
            .query_param("where", "registre=\"552100554\"");
        then.status(500);
    });
    let holders = server.mock(|when, then| {
        when.method(GET)
            .path("/decp.test/data/")
            .query_param("titulaire_id__contains", "552100554")
            .query_param("dateNotification__sort", "desc");
        then.status(200)
            .header("content-type", "application/json")
            .body(HOLDER);
    });
    let usecase = LodestoneUseCase::new(pool.clone());
    let modules = modules(&server);
    let clock = StepClock(AtomicI64::new(0));
    let settings = InspectionRunSettings { batch_size: 10 };
    let registry = [
        EcosystemModule::OfficialIdentity,
        EcosystemModule::LegalLife,
        EcosystemModule::PublicContracts,
    ];
    let runners: Vec<_> = modules
        .runners()
        .into_iter()
        .filter(|runner| registry.contains(&runner.module()))
        .collect();

    let first = usecase
        .inspect_due_domains(&runners, settings, &clock)
        .await
        .unwrap();
    assert_eq!(first.domains, 3);
    assert_eq!(first.failed, 1);
    assert_eq!(register.hits(), 1);
    assert_eq!(failing.hits(), 1);
    assert_eq!(holders.hits(), 1);
    let failed = state_row(&pool, EcosystemModule::LegalLife).await;
    assert_eq!(failed.status, "failed");
    assert!(failed.error.unwrap().contains("BODACC answered 500"));
    assert_eq!(failed.next_run_at, clock.now() + ChronoDuration::hours(6));
    let contracts = state_row(&pool, EcosystemModule::PublicContracts).await;
    assert_eq!(contracts.status, "done");
    assert_eq!(
        contracts.next_run_at,
        clock.now() + ChronoDuration::days(14)
    );

    failing.delete();
    let announcements = server.mock(|when, then| {
        when.method(GET)
            .path("/bodacc.test/datasets/annonces-commerciales/records")
            .query_param("where", "registre=\"552100554\"")
            .query_param("order_by", "dateparution desc")
            .query_param("limit", "20");
        then.status(200)
            .header("content-type", "application/json")
            .body(ANNOUNCEMENTS);
    });
    clock.0.store(7, Ordering::SeqCst);
    let retried = usecase
        .inspect_due_domains(&runners, settings, &clock)
        .await
        .unwrap();
    assert_eq!(retried.domains, 1);
    assert_eq!(announcements.hits(), 1);
    assert_eq!(register.hits(), 1);
    let done = state_row(&pool, EcosystemModule::LegalLife).await;
    assert_eq!(done.status, "done");
    assert_eq!(done.next_run_at, clock.now() + ChronoDuration::days(7));

    let lead = with_tx(&pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(DOMAIN).unwrap())
            .await
    })
    .await
    .unwrap()
    .unwrap();
    let detail = usecase.lead_detail(lead.id).await.unwrap();
    let identity: Vec<(&str, &str)> = detail
        .ecosystem
        .findings_of(EcosystemModule::OfficialIdentity)
        .map(|found| (found.kind.as_str(), found.source_url.as_str()))
        .collect();
    assert!(identity.contains(&("siren", "https://acme-logiciels.fr/mentions-legales")));
    assert!(identity.contains(&(
        "legal_name",
        "https://recherche-entreprises.api.gouv.fr/search?q=552100554"
    )));
    let legal: Vec<&str> = detail
        .ecosystem
        .findings_of(EcosystemModule::LegalLife)
        .map(|found| found.kind.as_str())
        .collect();
    assert_eq!(
        legal,
        [
            "collective_proceeding",
            "accounts_filing",
            "modification",
            "sale",
            "deregistration",
            "creation"
        ]
    );
    let awards = detail
        .ecosystem
        .findings_of(EcosystemModule::PublicContracts)
        .count();
    assert_eq!(awards, 3);
    assert_eq!(
        detail.ecosystem.state_of(EcosystemModule::LegalLife).status,
        ModuleStatus::Done
    );
}

fn greenhouse_board(count: usize) -> Value {
    let jobs: Vec<Value> = (0..count)
        .map(|index| {
            json!({
                "id": index,
                "title": format!("Backend Engineer {index}"),
                "absolute_url": format!("https://boards.greenhouse.io/acme/jobs/{index}"),
                "content": "Rust",
                "location": {"name": "Lyon"}
            })
        })
        .collect();
    json!({ "jobs": jobs })
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_hiring_module_compares_its_count_with_the_previous_pass(pool: PgPool) {
    let profile = active_profile(&pool).await;
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(GET).path(format!("/{DOMAIN}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(format!(
                "{HOME}<a href=\"https://boards.greenhouse.io/acme\">Carrières</a>"
            ));
    });
    let mut board = server.mock(|when, then| {
        when.method(GET)
            .path("/boards-api.greenhouse.io/v1/boards/acme/jobs")
            .query_param("content", "true");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(greenhouse_board(3));
    });
    let usecase = LodestoneUseCase::new(pool.clone());
    let modules = modules(&server);
    let clock = StepClock(AtomicI64::new(0));
    let settings = InspectionRunSettings { batch_size: 10 };
    let runners: Vec<_> = modules
        .runners()
        .into_iter()
        .filter(|runner| runner.module() == EcosystemModule::HiringActivity)
        .collect();

    let first = usecase
        .inspect_due_domains(&runners, settings, &clock)
        .await
        .unwrap();
    assert_eq!(first.domains, 1);
    board.delete();
    server.mock(|when, then| {
        when.method(GET)
            .path("/boards-api.greenhouse.io/v1/boards/acme/jobs")
            .query_param("content", "true");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(greenhouse_board(5));
    });
    clock.0.store(24 * 4, Ordering::SeqCst);
    let second = usecase
        .inspect_due_domains(&runners, settings, &clock)
        .await
        .unwrap();
    assert_eq!(second.domains, 1);

    let lead = with_tx(&pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(DOMAIN).unwrap())
            .await
    })
    .await
    .unwrap()
    .unwrap();
    let detail = usecase.lead_detail(lead.id).await.unwrap();
    let summary = detail
        .ecosystem
        .findings_of(EcosystemModule::HiringActivity)
        .find(|found| found.kind == "hiring_summary")
        .unwrap();
    assert_eq!(
        summary.value,
        "5 offres ouvertes : 5 engineering (+2 depuis le dernier passage)"
    );
    assert_eq!(summary.details["previous_count"], 3);
    assert_eq!(
        summary.details["history"],
        json!([
            {"at": "2026-10-08", "count": 3},
            {"at": "2026-10-12", "count": 5}
        ])
    );
}
