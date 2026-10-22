use chrono::NaiveDate;
use lodestone_core::{
    CompanyDossier, CompanyPage, DossierState, InspectionSettings, ModuleRunner, NormalizedDomain,
    OfficialData, Siren, SirenLookup,
};
use url::Url;

use super::{
    fake::{FakeFetcher, FakeResolver, domain, html, now, target},
    *,
};
use crate::endpoint::Endpoint;

const HOME: &str = "<html><head><title>Acme</title><meta name=\"generator\" content=\"WordPress 6.4\"></head><body><h1>Acme, logiciels pour les collectivités</h1><p>Nous construisons des outils de gestion depuis 2004.</p></body></html>";

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
        active: None,
        category: None,
        legal_nature: None,
        head_office_department: None,
    }
}

fn research(fetcher: &Arc<FakeFetcher>) -> CompanyResearch<FakeFetcher> {
    CompanyResearch::new(
        Arc::clone(fetcher),
        CompanySettings {
            entreprises_url: Endpoint::from(Url::parse("https://entreprises.test/search").unwrap()),
            max_pages: 3,
            max_rate_limit_wait: Duration::ZERO,
            origin: None,
        },
    )
}

fn built_dossier(domain: &NormalizedDomain) -> CompanyDossier {
    let mut dossier = CompanyDossier::pending(domain.clone(), now());
    dossier.state = DossierState::Complete;
    dossier.built_at = Some(now());
    dossier
}

fn found(inspection: Inspection) -> Vec<EcosystemFinding> {
    match inspection {
        Inspection::Found(findings) => findings,
        Inspection::NotApplicable(reason) => panic!("not applicable: {reason}"),
    }
}

#[tokio::test]
async fn the_identity_module_reuses_a_built_dossier_without_fetching() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    let module = IdentityModule::new(research(&fetcher));
    let domain = domain();
    let mut dossier = built_dossier(&domain);
    dossier.official = Some(official());
    dossier.siren = Some(official().siren);
    dossier.pages = vec![
        CompanyPage {
            url: "https://acme.fr/".to_owned(),
            text: "Acme, logiciels".to_owned(),
        },
        CompanyPage {
            url: "https://acme.fr/mentions-legales".to_owned(),
            text: "ACME SAS, RCS Lyon 552 100 554".to_owned(),
        },
    ];
    let settings = InspectionSettings::default();
    let findings = found(
        module
            .inspect(&target(&domain, &settings, Some(&dossier)))
            .await
            .unwrap(),
    );
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
    assert_eq!(findings[1].key, "siren");
    assert_eq!(findings[1].source_url, "https://acme.fr/mentions-legales");
    assert!(findings[2..].iter().all(|found| {
        found
            .source_url
            .starts_with("https://recherche-entreprises")
    }));
}

#[tokio::test]
async fn the_identity_module_gives_the_creation_date_and_the_siren_alone_without_register_data() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    let module = IdentityModule::new(research(&fetcher));
    let domain = domain();
    let settings = InspectionSettings::default();
    let mut dossier = built_dossier(&domain);
    dossier.siren = Some(official().siren);
    let findings = found(
        module
            .inspect(&target(&domain, &settings, Some(&dossier)))
            .await
            .unwrap(),
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].value, "552100554");
    assert_eq!(
        findings[0].source_url,
        "https://recherche-entreprises.api.gouv.fr/search?q=552100554"
    );
    let mut dated = official();
    dated.created_on = NaiveDate::from_ymd_opt(2004, 6, 9);
    let mut complete = built_dossier(&domain);
    complete.official = Some(dated);
    let findings = found(
        module
            .inspect(&target(&domain, &settings, Some(&complete)))
            .await
            .unwrap(),
    );
    let created = findings
        .iter()
        .find(|found| found.kind == "created_on")
        .unwrap();
    assert_eq!(created.label, "Date de création");
    assert_eq!(created.value, "2004-06-09");
}

#[tokio::test]
async fn the_identity_module_is_not_applicable_without_siren() {
    let fetcher = Arc::new(FakeFetcher::new(Vec::new()));
    let module = IdentityModule::new(research(&fetcher));
    let domain = domain();
    let settings = InspectionSettings::default();
    let dossier = built_dossier(&domain);
    let inspection = module
        .inspect(&target(&domain, &settings, Some(&dossier)))
        .await
        .unwrap();
    assert_eq!(inspection, Inspection::NotApplicable("SIREN inconnu"));
    assert!(fetcher.requested().is_empty());
}

#[tokio::test]
async fn the_runner_enum_dispatches_to_each_module() {
    let fetcher = Arc::new(FakeFetcher::new(vec![("https://acme.fr/", html(HOME))]));
    let modules = EcosystemModules {
        identity: IdentityModule::new(research(&fetcher)),
        legal: LegalLifeModule::new(
            Arc::clone(&fetcher),
            BodaccSettings {
                dataset_url: Endpoint::from(Url::parse("https://bodacc.test/dataset").unwrap()),
                max_rate_limit_wait: Duration::ZERO,
            },
            research(&fetcher),
        ),
        contracts: PublicContractsModule::new(
            Arc::clone(&fetcher),
            HolderSettings {
                holders_url: Endpoint::from(Url::parse("https://decp.test/data/").unwrap()),
                max_rate_limit_wait: Duration::ZERO,
            },
            research(&fetcher),
        ),
        website: WebsiteModule::new(
            Arc::clone(&fetcher),
            WebsiteSettings {
                max_rate_limit_wait: Duration::ZERO,
                origin: None,
            },
        ),
        dns: DnsEmailModule::new(Arc::new(FakeResolver::default())),
        hiring: HiringModule::new(AtsScanner::new(Arc::clone(&fetcher), 1, Duration::ZERO)),
        recent: RecentModule::new(
            Arc::clone(&fetcher),
            WebsiteSettings {
                max_rate_limit_wait: Duration::ZERO,
                origin: None,
            },
        ),
    };
    let runners = modules.runners();
    let names: Vec<EcosystemModule> = runners.iter().map(ModuleRunner::module).collect();
    assert_eq!(names, EcosystemModule::AVAILABLE);
    let settings = InspectionSettings::default();
    let domain = domain();
    let findings = found(
        runners[3]
            .inspect(&target(&domain, &settings, None))
            .await
            .unwrap(),
    );
    assert_eq!(findings[0].kind, "website");
    let dns = found(
        runners[4]
            .inspect(&target(&domain, &settings, None))
            .await
            .unwrap(),
    );
    assert_eq!(dns[0].kind, "no_mx");
    assert_eq!(dns[0].module, EcosystemModule::DnsEmail);
    assert_eq!(
        runners[6]
            .inspect(&target(&domain, &settings, None))
            .await
            .unwrap(),
        Inspection::Found(Vec::new())
    );
    let mut unknown = target(&domain, &settings, None);
    unknown.siren = SirenLookup::Unknown;
    for runner in &runners[1..3] {
        assert_eq!(
            runner.inspect(&unknown).await.unwrap(),
            Inspection::NotApplicable("SIREN inconnu")
        );
    }
}
