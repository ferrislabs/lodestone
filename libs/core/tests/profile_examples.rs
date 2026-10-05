use std::{fs, path::PathBuf};

use lodestone_core::{
    SourceName, ValidatedProfile,
    definition::{AgentSource, ProfileDefinition},
};
use schemars::schema_for;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const IAM_V1: &str = include_str!("fixtures/profiles/iam-ferriskey.v1.json");
const ECOMMERCE: &str =
    include_str!("../../../docs/examples/profiles/ecommerce-replatforming.json");

fn parse(raw: &str) -> ValidatedProfile {
    let value: serde_json::Value = serde_json::from_str(raw).expect("example is valid JSON");
    match ValidatedProfile::parse(value) {
        Ok(profile) => profile,
        Err(error) => panic!("{}: {:?}", error.path, error.problem),
    }
}

fn agent_ids(profile: &ValidatedProfile) -> Vec<&str> {
    profile
        .definition()
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect()
}

#[test]
fn iam_example_is_valid_and_has_expected_content() {
    let profile = parse(IAM);
    let definition = profile.definition();
    let labels = &definition
        .enabled_agents(SourceName::CertificateTransparency)
        .find_map(|agent| agent.source.certificate_transparency())
        .expect("iam example enables certificate transparency")
        .labels;
    for expected in [
        "auth", "sso", "login", "iam", "keycloak", "id", "accounts", "identity", "idp",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected}"
        );
    }
    let probe = definition
        .enabled_agents(SourceName::HttpProbe)
        .find_map(|agent| agent.source.http_probe())
        .expect("iam example has a probe");
    assert_eq!(probe.excluded_products, ["ferriskey"]);
    assert_eq!(probe.fingerprints.len(), 5);
}

#[test]
fn iam_example_has_two_job_agents_with_their_own_criteria() {
    let profile = parse(IAM);
    let ats: Vec<_> = profile
        .definition()
        .agents
        .iter()
        .filter(|agent| matches!(agent.source, AgentSource::Ats(_)))
        .collect();
    assert_eq!(
        ats.iter()
            .map(|agent| agent.id.as_str())
            .collect::<Vec<_>>(),
        ["ats-fr", "ats-en"]
    );
    assert!(ats[0].llm.is_none());
    assert!(
        ats[1]
            .llm
            .as_ref()
            .and_then(|llm| llm.relevance_criteria.as_deref())
            .is_some()
    );
    assert_eq!(
        profile
            .definition()
            .enabled_keyword_rules(SourceName::Ats)
            .next()
            .expect("first ats agent")
            .keywords[0],
        "Keycloak"
    );
}

#[test]
fn ecommerce_example_is_valid_without_iam_fields() {
    let profile = parse(ECOMMERCE);
    let definition = profile.definition();
    assert_eq!(
        definition
            .enabled_agents(SourceName::CertificateTransparency)
            .count(),
        0
    );
    assert_eq!(definition.enabled_agents(SourceName::DnsCname).count(), 0);
}

#[test]
fn examples_round_trip_through_json() {
    for raw in [IAM, ECOMMERCE] {
        let profile = parse(raw);
        assert_eq!(parse_value(profile.to_json()), profile);
    }
}

fn parse_value(value: serde_json::Value) -> ValidatedProfile {
    ValidatedProfile::parse(value).expect("round trip parses")
}

#[test]
fn version_one_iam_fixture_is_upgraded_to_the_version_two_example() {
    let upgraded = parse(IAM_V1);
    let current = parse(IAM);
    assert_eq!(upgraded.definition().schema_version, 2);
    assert_eq!(
        agent_ids(&upgraded),
        [
            "certificate-transparency",
            "http-probe",
            "dns-cname",
            "boamp",
            "decp",
            "ted",
            "france-travail",
            "ats"
        ]
    );
    let names: Vec<_> = upgraded
        .definition()
        .agents
        .iter()
        .map(|agent| agent.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Certificate transparency",
            "HTTP probe",
            "DNS CNAME",
            "BOAMP",
            "DECP",
            "TED",
            "France Travail",
            "ATS"
        ]
    );
    assert!(
        upgraded
            .definition()
            .agents
            .iter()
            .all(|agent| agent.enabled && agent.llm.is_none())
    );
    let strip = |profile: &ValidatedProfile| {
        let mut value = profile.to_json();
        let agents = value["agents"].as_array_mut().expect("agents");
        agents.retain(|agent| agent["id"] != "ats-en");
        for agent in agents.iter_mut() {
            if agent["id"] == "ats-fr" {
                agent["id"] = "ats".into();
                agent["name"] = "ATS".into();
            }
        }
        value
    };
    assert_eq!(strip(&upgraded), strip(&current));
    assert_eq!(upgraded.definition().llm, current.definition().llm);
    assert_eq!(upgraded.definition().scoring, current.definition().scoring);
}

#[test]
fn committed_schema_matches_definition() {
    let mut generated =
        serde_json::to_string_pretty(&schema_for!(ProfileDefinition)).expect("schema serializes");
    generated.push('\n');
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas/profile.v2.schema.json");
    if std::env::var("UPDATE_SCHEMA").as_deref() == Ok("1") {
        fs::write(&path, &generated).expect("schema file is writable");
        return;
    }
    let committed =
        fs::read_to_string(&path).expect("schema file exists, regenerate with UPDATE_SCHEMA=1");
    assert!(
        committed == generated,
        "docs/schemas/profile.v2.schema.json is stale, regenerate with UPDATE_SCHEMA=1"
    );
}
