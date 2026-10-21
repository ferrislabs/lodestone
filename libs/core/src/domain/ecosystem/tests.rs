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
    assert_eq!(
        EcosystemModule::AVAILABLE,
        [
            EcosystemModule::OfficialIdentity,
            EcosystemModule::WebsiteStack,
            EcosystemModule::HiringActivity
        ]
    );
    assert!(EcosystemModule::WebsiteStack.reads_site());
    assert!(!EcosystemModule::LegalLife.reads_site());
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
}
