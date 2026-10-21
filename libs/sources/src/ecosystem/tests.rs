use std::{collections::HashMap, sync::Mutex};

use chrono::TimeZone;
use lodestone_core::{
    CompanyDossier, DossierState, InspectionSettings, ModuleRunner, OfficialData, Siren,
    definition::Fingerprint,
};
use serde_json::{Value, from_value};

use super::*;
use crate::endpoint::Endpoint;

type Reply = Result<(u16, Vec<(&'static str, &'static str)>, String), FetchError>;

struct FakeFetcher {
    script: HashMap<String, Reply>,
    requests: Mutex<Vec<String>>,
}

impl FakeFetcher {
    fn new(script: Vec<(&str, Reply)>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|(url, reply)| (url.to_owned(), reply))
                .collect(),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requested(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetcher for FakeFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let key = request.url.to_string();
        self.requests.lock().unwrap().push(key.clone());
        match self.script.get(&key) {
            Some(Ok((status, headers, body))) => Ok(FetchResponse {
                status: *status,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
                body: body.clone().into_bytes(),
                final_url: request.url.clone(),
                truncated: false,
            }),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(FetchResponse {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
                final_url: request.url.clone(),
                truncated: false,
            }),
        }
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
}

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

const HOME: &str = "<html><head><title>Acme</title><meta name=\"generator\" content=\"WordPress 6.4\"></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Nous construisons des outils de gestion depuis 2004.</p></body></html>";

fn html(body: &str) -> Reply {
    Ok((
        200,
        vec![("content-type", "text/html"), ("server", "nginx")],
        body.to_owned(),
    ))
}

fn fingerprint(value: Value) -> Fingerprint {
    from_value(value).unwrap()
}

fn website(fetcher: FakeFetcher) -> WebsiteModule<FakeFetcher> {
    WebsiteModule::new(
        Arc::new(fetcher),
        WebsiteSettings {
            max_rate_limit_wait: Duration::ZERO,
            origin: None,
        },
    )
}

fn target<'a>(
    domain: &'a NormalizedDomain,
    settings: &'a InspectionSettings,
    dossier: Option<&'a CompanyDossier>,
) -> InspectionTarget<'a> {
    InspectionTarget {
        domain,
        settings,
        dossier,
        now: now(),
    }
}

#[tokio::test]
async fn the_website_module_reads_the_home_page_and_the_profile_fingerprints() {
    let module = website(FakeFetcher::new(vec![
        ("https://acme.fr/", html(HOME)),
        (
            "https://auth.acme.fr/realms/master",
            Ok((
                200,
                vec![("x-keycloak-version", "21.1.2")],
                "{\"realm\":\"master\"}".to_owned(),
            )),
        ),
    ]));
    let settings = InspectionSettings {
        fingerprints: vec![fingerprint(json!({
            "id": "keycloak-realm",
            "path": "/realms/master",
            "product": "keycloak",
            "matchers": [{"kind": "status", "equals": 200}],
            "version": {"from": "header", "name": "x-keycloak-version", "pattern": "^(\\d+\\.\\d+)"},
            "debt_markers": [{"id": "old_keycloak", "rule": {"kind": "version_below", "version": "22.0"}}]
        }))],
        excluded_products: Vec::new(),
        extra_subdomains: vec!["auth".to_owned()],
        highlight_keywords: Vec::new(),
    };
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("website", "https://acme.fr/"),
            ("server", "nginx"),
            ("generator", "WordPress 6.4"),
            ("product", "keycloak 21.1"),
            ("debt_marker", "old_keycloak"),
        ]
    );
    assert_eq!(findings[3].source_url, "https://auth.acme.fr/realms/master");
    assert!(
        findings
            .iter()
            .all(|found| found.module == EcosystemModule::WebsiteStack)
    );
}

#[tokio::test]
async fn excluded_products_and_unconfirmed_pages_leave_no_finding() {
    let module = website(FakeFetcher::new(vec![(
        "https://acme.fr/realms/master",
        Ok((200, Vec::new(), "ok".to_owned())),
    )]));
    let settings = InspectionSettings {
        fingerprints: vec![fingerprint(json!({
            "id": "ferriskey",
            "path": "/realms/master",
            "product": "ferriskey",
            "matchers": [{"kind": "status", "equals": 200}]
        }))],
        excluded_products: vec!["ferriskey".to_owned()],
        extra_subdomains: Vec::new(),
        highlight_keywords: Vec::new(),
    };
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert!(findings.is_empty());
}

#[tokio::test]
async fn a_rate_limited_home_page_stops_the_module() {
    let module = website(FakeFetcher::new(vec![(
        "https://acme.fr/",
        Err(FetchError::RateLimited {
            retry_after: Duration::from_secs(30),
        }),
    )]));
    let settings = InspectionSettings::default();
    let domain = domain();
    let error = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap_err();
    assert!(matches!(error, SourceError::RateLimited { .. }));
}

#[tokio::test]
async fn the_hiring_module_lists_the_board_and_highlights_offers() {
    let board = json!({"jobs": [
        {"id": 1, "title": "Comptable", "absolute_url": "https://boards.greenhouse.io/acme/jobs/1", "content": "Comptabilité"},
        {"id": 2, "title": "Ingénieur IAM", "absolute_url": "https://boards.greenhouse.io/acme/jobs/2", "content": "Keycloak et SSO", "location": {"name": "Lyon"}}
    ]});
    let fetcher = FakeFetcher::new(vec![
        (
            "https://acme.fr/",
            html(&format!(
                "{HOME}<a href=\"https://boards.greenhouse.io/acme\">Carrières</a>"
            )),
        ),
        (
            "https://boards-api.greenhouse.io/v1/boards/acme/jobs?content=true",
            Ok((
                200,
                vec![("content-type", "application/json")],
                board.to_string(),
            )),
        ),
    ]);
    let module = HiringModule::new(AtsScanner::new(Arc::new(fetcher), 2, Duration::ZERO));
    let settings = InspectionSettings {
        highlight_keywords: vec!["keycloak".to_owned()],
        ..InspectionSettings::default()
    };
    let domain = domain();
    let findings = module
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("careers_page", "https://acme.fr/"),
            ("ats_board", "greenhouse (acme)"),
            ("open_positions", "2"),
            ("job_offer", "Ingénieur IAM"),
            ("job_offer", "Comptable"),
        ]
    );
    assert_eq!(findings[3].details["highlighted"], true);
    assert_eq!(findings[3].details["keywords"], json!(["keycloak"]));
    assert_eq!(findings[3].details["location"], "Lyon");
    assert_eq!(findings[4].details["highlighted"], false);
    assert_eq!(
        findings[3].source_url,
        "https://boards.greenhouse.io/acme/jobs/2"
    );
}

fn official() -> OfficialData {
    OfficialData {
        siren: Siren::parse("552100554").unwrap(),
        legal_name: "ACME SAS".to_owned(),
        naf_code: Some("62.01Z".to_owned()),
        naf_label: Some("Information et communication".to_owned()),
        headcount_code: Some("12".to_owned()),
        headcount_range: Some("20 à 49 salariés".to_owned()),
        head_office_city: Some("LYON".to_owned()),
        head_office_postcode: Some("69002".to_owned()),
        head_office_country: Some("France".to_owned()),
        head_office_country_code: Some("FR".to_owned()),
        created_on: None,
    }
}

#[tokio::test]
async fn the_identity_module_reuses_a_built_dossier_without_fetching() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    let research = CompanyResearch::new(
        Arc::clone(&fetcher),
        CompanySettings {
            entreprises_url: Endpoint::from(Url::parse("https://entreprises.test/search").unwrap()),
            max_pages: 3,
            max_rate_limit_wait: Duration::ZERO,
            origin: None,
        },
    );
    let module = IdentityModule::new(research);
    let domain = domain();
    let mut dossier = CompanyDossier::pending(domain.clone(), now());
    dossier.state = DossierState::Complete;
    dossier.official = Some(official());
    dossier.built_at = Some(now());
    let settings = InspectionSettings::default();
    let findings = module
        .inspect(&target(&domain, &settings, Some(&dossier)))
        .await
        .unwrap();
    assert!(fetcher.requested().is_empty());
    let pairs: Vec<(&str, &str)> = findings
        .iter()
        .map(|found| (found.kind.as_str(), found.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("legal_name", "ACME SAS"),
            ("siren", "552100554"),
            ("activity", "62.01Z, Information et communication"),
            ("headcount", "20 à 49 salariés"),
            ("head_office", "69002 LYON France"),
        ]
    );
    assert_eq!(
        findings[0].source_url,
        "https://recherche-entreprises.api.gouv.fr/search?q=552100554"
    );
}

#[tokio::test]
async fn the_runner_enum_dispatches_to_each_module() {
    let fetcher = Arc::new(FakeFetcher::new(vec![("https://acme.fr/", html(HOME))]));
    let modules = EcosystemModules {
        identity: IdentityModule::new(CompanyResearch::new(
            Arc::clone(&fetcher),
            CompanySettings {
                entreprises_url: Endpoint::from(
                    Url::parse("https://entreprises.test/search").unwrap(),
                ),
                max_pages: 1,
                max_rate_limit_wait: Duration::ZERO,
                origin: None,
            },
        )),
        website: WebsiteModule::new(
            Arc::clone(&fetcher),
            WebsiteSettings {
                max_rate_limit_wait: Duration::ZERO,
                origin: None,
            },
        ),
        hiring: HiringModule::new(AtsScanner::new(Arc::clone(&fetcher), 1, Duration::ZERO)),
    };
    let runners = modules.runners();
    let names: Vec<EcosystemModule> = runners.iter().map(ModuleRunner::module).collect();
    assert_eq!(names, EcosystemModule::AVAILABLE);
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = runners[1]
        .inspect(&target(&domain, &settings, None))
        .await
        .unwrap();
    assert_eq!(findings[0].kind, "website");
}
