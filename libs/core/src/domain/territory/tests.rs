use chrono::{TimeZone, Utc};
use serde_json::{Value, from_str, json};
use uuid::Uuid;

use super::*;
use crate::domain::{
    lead::{LeadId, SignalId},
    profile::{
        ValidatedProfile,
        definition::{AgentId, ProfileDefinition},
        fixtures::MINIMAL_PROFILE,
    },
    values::{Provenance, SignalKind, SourceName, UnitInterval},
};

fn domain(raw: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(raw).unwrap()
}

fn france() -> Vec<CountryCode> {
    vec![CountryCode::new("FR")]
}

fn definition() -> ProfileDefinition {
    let mut value = ValidatedProfile::parse(from_str(MINIMAL_PROFILE).unwrap())
        .unwrap()
        .to_json();
    value["agents"] = json!([
        {"type": "certificates", "id": "certificats", "name": "Certificats", "labels": ["auth"], "countries": ["FR"]},
        {"type": "dns", "id": "dns", "name": "DNS", "patterns": [{"provider": "okta", "target_suffix": "okta.com"}]}
    ]);
    ValidatedProfile::parse(value).unwrap().definition().clone()
}

fn signal(agent: Option<&str>) -> Signal {
    Signal {
        id: SignalId(Uuid::nil()),
        lead_id: LeadId(Uuid::nil()),
        kind: SignalKind::parse("certificate_seen").unwrap(),
        value: None,
        source: SourceName::CertificateTransparency,
        raw: Value::Null,
        provenance: Provenance::parse("https://crt.sh/?q=auth.acme.com").unwrap(),
        strength: UnitInterval::new(0.3).unwrap(),
        observed_at: Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap(),
        agent: agent.map(AgentId::new),
    }
}

#[test]
fn a_country_code_tld_is_kept_when_listed_and_dropped_otherwise() {
    assert_eq!(
        admit_host(&domain("acme.fr"), &france(), None),
        HostAdmission::Admitted
    );
    assert_eq!(
        admit_host(&domain("acme.de"), &france(), None),
        HostAdmission::OutOfCountry
    );
    assert_eq!(
        admit_host(&domain("acme.de"), &france(), Some("FR")),
        HostAdmission::OutOfCountry
    );
    assert_eq!(
        admit_host(&domain("acme.fr"), &france(), Some("DE")),
        HostAdmission::Admitted
    );
    assert_eq!(
        territory_of(&domain("acme.co.uk"), &france()),
        Territory::Foreign
    );
}

#[test]
fn a_generic_tld_is_kept_until_the_head_office_is_known_elsewhere() {
    assert_eq!(
        admit_host(&domain("acme.com"), &france(), None),
        HostAdmission::Admitted
    );
    assert_eq!(
        admit_host(&domain("acme.com"), &france(), Some("fr")),
        HostAdmission::Admitted
    );
    assert_eq!(
        admit_host(&domain("acme.com"), &france(), Some("US")),
        HostAdmission::OutOfCountry
    );
}

#[test]
fn without_countries_every_host_is_admitted_but_saas_tenants() {
    assert_eq!(
        admit_host(&domain("acme.de"), &[], None),
        HostAdmission::Admitted
    );
    assert_eq!(
        admit_host(&domain("acme.okta.com"), &[], None),
        HostAdmission::SaasTenant
    );
    assert_eq!(
        admit_host(&domain("salesforce.com"), &france(), None),
        HostAdmission::SaasTenant
    );
}

#[test]
fn a_generic_lead_found_only_by_filtered_agents_is_out_of_country() {
    let definition = definition();
    let lead = domain("acme.com");
    let filtered = [signal(Some("certificats")), signal(None)];
    assert!(out_of_country(&definition, &lead, &filtered, "US"));
    assert!(!out_of_country(&definition, &lead, &filtered, "FR"));
    let mixed = [signal(Some("certificats")), signal(Some("dns"))];
    assert!(!out_of_country(&definition, &lead, &mixed, "US"));
    let unknown = [signal(Some("retired"))];
    assert!(!out_of_country(&definition, &lead, &unknown, "US"));
    assert!(!out_of_country(&definition, &lead, &[signal(None)], "US"));
    assert!(!out_of_country(
        &definition,
        &domain("acme.fr"),
        &filtered,
        "US"
    ));
}

#[test]
fn the_reason_is_in_french() {
    assert_eq!(OUT_OF_COUNTRY_REASON, "hors pays visé");
}
