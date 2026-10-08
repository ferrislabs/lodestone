use chrono::TimeZone;
use serde_json::{from_str, from_value, json, to_value};

use super::*;
use crate::domain::{
    company::{CompanyDossier, DossierState},
    profile::{ValidatedProfile, fixtures::MINIMAL_PROFILE},
};

fn at(minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, 9, minute, 0).unwrap()
}

fn domain(raw: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(raw).unwrap()
}

#[test]
fn every_platform_of_the_list_is_detected_on_its_subdomains_only() {
    assert_eq!(HOSTING_PLATFORMS.len(), 18);
    for platform in HOSTING_PLATFORMS {
        let site = format!("acme.{platform}");
        let Ok(parsed) = NormalizedDomain::from_host(&site) else {
            continue;
        };
        assert_eq!(hosting_platform(&parsed), Some(platform), "{site}");
    }
    assert_eq!(
        hosting_platform(&domain("docs.acme.notion.site")),
        Some("notion.site")
    );
    assert_eq!(
        hosting_platform(&domain("acme.github.io")),
        Some("github.io")
    );
    assert_eq!(hosting_platform(&domain("acme.fr")), None);
    assert_eq!(hosting_platform(&domain("notgithub.io")), None);
    assert_eq!(hosting_platform(&domain("myvercel.app")), None);
}

#[test]
fn every_module_has_french_copy_and_a_stable_key() {
    for module in EcosystemModule::ALL {
        assert_eq!(EcosystemModule::parse(module.as_str()), Some(module));
        assert!(!module.label().is_empty());
        assert!(module.question().ends_with('?'));
        assert!(module.learns().len() > 20);
        assert!(module.refresh_interval() > Duration::zero());
    }
    assert_eq!(EcosystemModule::AVAILABLE, EcosystemModule::ALL);
    assert!(
        EcosystemModule::ALL
            .into_iter()
            .all(EcosystemModule::is_available)
    );
    assert_eq!(
        EcosystemModule::LegalLife.refresh_interval(),
        Duration::days(7)
    );
    assert_eq!(
        EcosystemModule::PublicContracts.refresh_interval(),
        Duration::days(14)
    );
    assert!(EcosystemModule::LegalLife.needs_siren());
    assert!(EcosystemModule::PublicContracts.needs_siren());
    assert!(EcosystemModule::OfficialIdentity.needs_siren());
    assert!(!EcosystemModule::WebsiteStack.needs_siren());
    assert!(EcosystemModule::WebsiteStack.reads_site());
    assert!(EcosystemModule::HiringActivity.reads_site());
    assert!(EcosystemModule::RecentActivity.reads_site());
    assert!(!EcosystemModule::LegalLife.reads_site());
    assert!(!EcosystemModule::DnsEmail.reads_site());
    assert!(EcosystemModule::HiringActivity.keeps_history());
    assert!(!EcosystemModule::WebsiteStack.keeps_history());
    assert!(!ModuleStatus::Pending.is_settled());
    assert!(ModuleStatus::NotApplicable.is_settled());
}

#[test]
fn findings_round_trip_through_json() {
    let finding = EcosystemFinding::new(
        EcosystemModule::WebsiteStack,
        "server",
        "Serveur web",
        "nginx",
        "https://acme.fr/",
        at(1),
    )
    .with_details(json!({"header": "server"}));
    assert_eq!(finding.key, "server:nginx");
    let value = to_value(&finding).unwrap();
    assert_eq!(value["module"], "website_stack");
    let back: EcosystemFinding = from_value(value).unwrap();
    assert_eq!(back, finding);
}

#[test]
fn documents_group_findings_by_module_and_source() {
    let findings = [
        EcosystemFinding::new(
            EcosystemModule::OfficialIdentity,
            "legal_name",
            "Raison sociale",
            "ACME SAS",
            "https://recherche-entreprises.api.gouv.fr/search?q=123456789",
            at(1),
        ),
        EcosystemFinding::new(
            EcosystemModule::OfficialIdentity,
            "siren",
            "SIREN",
            "123456789",
            "https://recherche-entreprises.api.gouv.fr/search?q=123456789",
            at(1),
        ),
        EcosystemFinding::new(
            EcosystemModule::HiringActivity,
            "job_offer",
            "Offre d'emploi",
            "Ingénieur IAM",
            "https://jobs.acme.fr/1",
            at(2),
        ),
    ];
    let documents = ecosystem_documents(&findings);
    assert_eq!(documents.len(), 2);
    assert!(
        documents[0]
            .content
            .starts_with("Ecosystem, module official identity (official_identity)")
    );
    assert!(documents[0].content.contains("- Raison sociale: ACME SAS"));
    assert!(documents[0].content.contains("- SIREN: 123456789"));
    assert_eq!(documents[1].source, "https://jobs.acme.fr/1");
}

#[test]
fn settings_merge_profiles_without_duplicates() {
    let mut first = ValidatedProfile::parse(from_str(MINIMAL_PROFILE).unwrap())
        .unwrap()
        .definition()
        .clone();
    first.enrichment.custom.extra_subdomains = vec!["auth".to_owned()];
    first.enrichment.highlight_keywords = vec!["SSO".to_owned()];
    let settings = InspectionSettings::of([&first, &first]);
    assert_eq!(settings.fingerprints.len(), 1);
    assert_eq!(settings.excluded_products, ["ferriskey"]);
    assert_eq!(settings.extra_subdomains, ["auth"]);
    assert_eq!(settings.highlight_keywords, ["SSO"]);
}

#[test]
fn a_module_state_defaults_to_pending() {
    let ecosystem = DomainEcosystem::default();
    assert_eq!(
        ecosystem.state_of(EcosystemModule::WebsiteStack),
        ModuleState::pending(EcosystemModule::WebsiteStack)
    );
    let dossier = CompanyDossier::pending(domain("acme.fr"), at(0));
    assert_eq!(dossier.state, DossierState::Pending);
    let outcome = ModuleOutcome::Failed("boom".to_owned());
    assert_eq!(outcome.status(), ModuleStatus::Failed);
    assert!(outcome.findings().is_empty());
    assert_eq!(outcome.note(), Some("boom"));
}

#[test]
fn a_module_without_siren_is_not_applicable_with_its_reason() {
    let outcome = Inspection::NotApplicable(SIREN_UNKNOWN).outcome();
    assert_eq!(outcome.status(), ModuleStatus::NotApplicable);
    assert_eq!(outcome.note(), Some("SIREN inconnu"));
    assert_eq!(ModuleOutcome::NotApplicable(None).note(), None);
    let found = Inspection::Found(Vec::new()).outcome();
    assert_eq!(found.status(), ModuleStatus::Done);
}

fn identity_finding(siren: &str) -> EcosystemFinding {
    EcosystemFinding::new(
        EcosystemModule::OfficialIdentity,
        "siren",
        "SIREN",
        siren,
        "https://acme.fr/mentions-legales",
        at(1),
    )
    .keyed(SIREN_FINDING_KEY)
}

#[test]
fn the_siren_comes_from_the_dossier_then_from_the_identity_findings() {
    let siren = Siren::parse("552100554").unwrap();
    let other = Siren::parse("732829320").unwrap();
    let mut dossier = CompanyDossier::pending(domain("acme.fr"), at(0));
    assert_eq!(SirenLookup::of(None, None, false), SirenLookup::Unresolved);
    assert_eq!(
        SirenLookup::of(Some(&dossier), None, false),
        SirenLookup::Unresolved
    );
    assert_eq!(SirenLookup::of(None, None, true), SirenLookup::Unknown);
    assert_eq!(
        SirenLookup::of(None, Some(&other), false),
        SirenLookup::Known(&other)
    );
    dossier.state = DossierState::Partial;
    dossier.built_at = Some(at(1));
    assert_eq!(
        SirenLookup::of(Some(&dossier), None, false),
        SirenLookup::Unknown
    );
    dossier.siren = Some(siren.clone());
    assert_eq!(
        SirenLookup::of(Some(&dossier), Some(&other), false),
        SirenLookup::Known(&siren)
    );
    let ecosystem = DomainEcosystem {
        modules: vec![ModuleState {
            module: EcosystemModule::OfficialIdentity,
            status: ModuleStatus::NotApplicable,
            last_run: Some(at(2)),
            changed_at: None,
            error: Some(SIREN_UNKNOWN.to_owned()),
        }],
        findings: vec![identity_finding("552 100 554")],
    };
    assert_eq!(ecosystem.recorded_siren(), Some(siren));
    assert!(ecosystem.identity_settled());
    assert!(!DomainEcosystem::default().identity_settled());
    assert_eq!(DomainEcosystem::default().recorded_siren(), None);
}

#[test]
fn legal_and_contract_findings_give_concise_document_lines() {
    let source = "https://bodacc-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/annonces-commerciales/records?where=registre%3D%22552100554%22";
    let findings = [
        EcosystemFinding::new(
            EcosystemModule::LegalLife,
            "collective_proceeding",
            "Procédure collective",
            "Jugement d'ouverture de liquidation judiciaire",
            source,
            at(1),
        )
        .with_details(json!({"date": "2026-09-10", "court": "Greffe du Tribunal de Nanterre"})),
        EcosystemFinding::new(
            EcosystemModule::LegalLife,
            "accounts_filing",
            "Dépôt des comptes",
            "Comptes annuels et rapports, clôture le 2025-12-31",
            source,
            at(1),
        )
        .with_details(json!({"date": "2026-04-02"})),
        EcosystemFinding::new(
            EcosystemModule::PublicContracts,
            "contract",
            "Marché remporté",
            "Diagnostic des réseaux d'eau potable",
            "https://tabular-api.data.gouv.fr/api/resources/r/data/?titulaire_id__contains=552100554",
            at(1),
        )
        .with_details(json!({
            "date": "2026-08-10",
            "buyer": "Communauté de communes du Pays d'Iroise",
            "amount_eur": 400000.0,
            "procedure": "Procédure adaptée"
        })),
    ];
    let documents = ecosystem_documents(&findings);
    assert_eq!(documents.len(), 2);
    assert_eq!(documents[0].source, source);
    assert!(
        documents[0]
            .content
            .starts_with("Ecosystem, module legal life (legal_life)")
    );
    assert!(documents[0].content.contains(
        "- Procédure collective (2026-09-10): Jugement d'ouverture de liquidation judiciaire; court Greffe du Tribunal de Nanterre"
    ));
    assert!(documents[0].content.contains(
        "- Dépôt des comptes (2026-04-02): Comptes annuels et rapports, clôture le 2025-12-31"
    ));
    assert!(documents[1].content.contains(
        "- Marché remporté (2026-08-10): Diagnostic des réseaux d'eau potable; buyer Communauté de communes du Pays d'Iroise; amount 400000 EUR; procedure Procédure adaptée"
    ));
}

#[test]
fn the_official_modules_depend_on_their_catalog_provider() {
    assert_eq!(
        EcosystemModule::OfficialIdentity.provider(),
        Some("company_research")
    );
    assert_eq!(EcosystemModule::LegalLife.provider(), Some("bodacc"));
    assert_eq!(EcosystemModule::PublicContracts.provider(), Some("decp"));
    assert_eq!(EcosystemModule::WebsiteStack.provider(), None);
    assert_eq!(EcosystemModule::HiringActivity.provider(), None);
    assert_eq!(EcosystemModule::DnsEmail.provider(), None);
    assert_eq!(EcosystemModule::RecentActivity.provider(), None);
    let readiness = EcosystemReadiness::with_not_ready(vec![EcosystemModule::LegalLife]);
    assert!(!readiness.is_ready(EcosystemModule::LegalLife));
    assert!(readiness.is_ready(EcosystemModule::PublicContracts));
    assert!(EcosystemReadiness::default().is_ready(EcosystemModule::LegalLife));
    assert_eq!(PROVIDER_NOT_READY, "Fournisseur non prêt");
}

#[test]
fn readiness_comes_from_the_ready_provider_keys() {
    let readiness = EcosystemReadiness::from_ready_providers(&[
        "company_research".to_owned(),
        "decp".to_owned(),
    ]);
    assert_eq!(
        readiness,
        EcosystemReadiness::with_not_ready(vec![EcosystemModule::LegalLife])
    );
    assert!(readiness.is_ready(EcosystemModule::WebsiteStack));
}

fn module_finding(module: EcosystemModule, index: usize) -> EcosystemFinding {
    EcosystemFinding::new(
        module,
        "technology",
        "Autre",
        format!("Outil {index}"),
        "https://acme.fr/",
        at(1),
    )
}

#[test]
fn the_website_document_keeps_more_lines_than_the_other_modules() {
    let findings: Vec<EcosystemFinding> = (0..40)
        .map(|index| module_finding(EcosystemModule::WebsiteStack, index))
        .chain((0..40).map(|index| module_finding(EcosystemModule::RecentActivity, index)))
        .collect();
    let documents = ecosystem_documents(&findings);
    let lines: Vec<usize> = documents
        .iter()
        .map(|document| document.content.lines().count() - 1)
        .collect();
    assert_eq!(
        lines,
        [
            EcosystemModule::WebsiteStack.document_limit(),
            MAX_DOCUMENT_FINDINGS
        ]
    );
    assert!(EcosystemModule::WebsiteStack.document_limit() > MAX_DOCUMENT_FINDINGS);
}

#[test]
fn the_new_modules_give_concise_agent_lines() {
    let lines: Vec<String> = [
        EcosystemFinding::new(
            EcosystemModule::DnsEmail,
            "mail_provider",
            "Messagerie",
            "Microsoft 365 (MX)",
            "dns:acme.fr?type=MX",
            at(1),
        ),
        EcosystemFinding::new(
            EcosystemModule::WebsiteStack,
            "technology",
            "CMS",
            "WordPress 6.4",
            "https://acme.fr/",
            at(1),
        ),
        EcosystemFinding::new(
            EcosystemModule::RecentActivity,
            "latest_news",
            "Dernière actualité",
            "Nouvelle version (2026-10-01)",
            "https://acme.fr/blog/nouvelle-version",
            at(1),
        ),
    ]
    .iter()
    .map(EcosystemFinding::document_line)
    .collect();
    assert_eq!(
        lines,
        [
            "- Messagerie: Microsoft 365 (MX)",
            "- CMS: WordPress 6.4",
            "- Dernière actualité: Nouvelle version (2026-10-01)",
        ]
    );
}

#[test]
fn saas_tenant_domains_and_their_subdomains_are_recognised() {
    assert_eq!(SAAS_TENANT_DOMAINS.len(), 20);
    for tenant in SAAS_TENANT_DOMAINS {
        assert!(saas_tenant(tenant).is_some(), "{tenant}");
        let host = format!("acme.{tenant}");
        assert!(saas_tenant(&host).is_some(), "{host}");
    }
    assert_eq!(
        saas_tenant("acme.my.salesforce.com"),
        Some("salesforce.com")
    );
    assert_eq!(saas_tenant("login.acme.okta.com"), Some("okta.com"));
    assert_eq!(saas_tenant("d111.cloudfront.net"), Some("cloudfront.net"));
    assert_eq!(saas_tenant("acme.fr"), None);
    assert_eq!(saas_tenant("myokta.com"), None);
    assert_eq!(saas_tenant("auth.acme.fr"), None);
    assert_eq!(SAAS_TENANT_REASON, "domaine d'un fournisseur SaaS");
}

#[test]
fn the_findings_hash_ignores_order_observation_time_and_pass_relative_values() {
    let module = EcosystemModule::RecentActivity;
    let freshness = |days: u32, latest: &str, minute: u32| {
        EcosystemFinding::new(
            module,
            "freshness",
            "Fraîcheur du site",
            format!("mis à jour il y a {days} jours ({latest})"),
            "https://acme.fr/sitemap.xml",
            at(minute),
        )
        .keyed("freshness")
        .with_details(json!({"days": days, "latest": latest}))
    };
    let news = EcosystemFinding::new(
        module,
        "news",
        "Actualité",
        "Lancement",
        "https://acme.fr/blog/lancement",
        at(1),
    );
    let first = findings_hash(&[freshness(3, "2026-10-05", 1), news.clone()]);
    let rerun = findings_hash(&[news.clone(), freshness(4, "2026-10-05", 9)]);
    assert_eq!(first, rerun);
    let changed = findings_hash(&[news.clone(), freshness(0, "2026-10-08", 9)]);
    assert_ne!(first, changed);
    let more = findings_hash(&[
        news.clone(),
        freshness(3, "2026-10-05", 1),
        EcosystemFinding::new(
            module,
            "news",
            "Actualité",
            "Levée",
            "https://acme.fr/b",
            at(2),
        ),
    ]);
    assert_ne!(first, more);
    assert_ne!(findings_hash(&[]), first);
    let summary = |count: u32, delta: i64| {
        EcosystemFinding::new(
            EcosystemModule::HiringActivity,
            "hiring_summary",
            "Recrutement",
            format!("{count} offres ouvertes (+{delta} depuis le dernier passage)"),
            "https://acme.fr/careers",
            at(1),
        )
        .keyed("hiring_summary")
        .with_details(json!({"count": count, "delta": delta, "previous_count": 1, "history": [1]}))
    };
    assert_eq!(
        findings_hash(&[summary(3, 2)]),
        findings_hash(&[summary(3, 0)])
    );
    assert_ne!(
        findings_hash(&[summary(3, 2)]),
        findings_hash(&[summary(4, 1)])
    );
}

#[test]
fn only_the_modules_reading_the_recorded_siren_wait_for_the_identity() {
    let waiting: Vec<EcosystemModule> = EcosystemModule::ALL
        .into_iter()
        .filter(|module| module.waits_for_identity())
        .collect();
    assert_eq!(
        waiting,
        [EcosystemModule::LegalLife, EcosystemModule::PublicContracts]
    );
}
