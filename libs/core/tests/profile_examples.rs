use std::{fs, path::PathBuf};

use lodestone_core::{
    SourceName, ValidatedProfile,
    definition::{AgentSource, ProfileDefinition},
};
use schemars::schema_for;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const IAM_V1: &str = include_str!("fixtures/profiles/iam-ferriskey.v1.json");
const IAM_V2: &str = include_str!("fixtures/profiles/iam-ferriskey.v2.json");
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
fn version_one_iam_fixture_is_upgraded_to_the_current_example() {
    let upgraded = parse(IAM_V1);
    let current = parse(IAM);
    assert_eq!(upgraded.definition().schema_version, 6);
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
        value["qualification_agents"] = serde_json::Value::Null;
        value["aggregator"] = serde_json::Value::Null;
        value["llm"]["qualification_threshold"] = 0.into();
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
    assert_eq!(
        upgraded.definition().llm.relevance_criteria,
        current.definition().llm.relevance_criteria
    );
    assert_eq!(upgraded.definition().scoring, current.definition().scoring);
    assert_eq!(upgraded.definition().llm.qualification_threshold, 37);
}

fn iam_weights(profile: &ValidatedProfile) -> Vec<(&str, u8)> {
    profile
        .definition()
        .scoring
        .factors
        .iter()
        .map(|factor| (factor.signal.as_str(), factor.weight))
        .collect()
}

#[test]
fn version_two_iam_fixture_is_upgraded_to_version_six() {
    let upgraded = parse(IAM_V2);
    let definition = upgraded.definition();
    assert_eq!(definition.schema_version, 6);
    assert_eq!(
        iam_weights(&upgraded),
        [
            ("product_detected", 10),
            ("debt_marker", 5),
            ("saas_provider", 3),
            ("job_offer", 8),
            ("public_tender", 7),
            ("role_contact_found", 2),
            ("distinct_signals", 2),
        ]
    );
    assert_eq!(definition.llm.qualification_threshold, 37);
    assert_eq!(definition.max_score(), 47.0);
    assert_eq!(upgraded.to_json()["schema_version"], 6);
    assert_eq!(parse(IAM_V1).definition().scoring, definition.scoring);
}

fn qualification_agent(profile: &ValidatedProfile, id: &str) -> Vec<(String, f64)> {
    profile
        .definition()
        .qualification_agents
        .iter()
        .find(|agent| agent.id.as_str() == id)
        .expect("agent exists")
        .questions[0]
        .options
        .iter()
        .map(|option| (option.value.clone(), option.score))
        .collect()
}

#[test]
fn iam_example_has_three_qualification_agents() {
    let profile = parse(IAM);
    let definition = profile.definition();
    let names: Vec<_> = definition
        .qualification_agents
        .iter()
        .map(|agent| (agent.id.as_str(), agent.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("pertinence", "Pertinence"),
            ("maturite-technique", "Maturité technique"),
            ("type-organisation", "Type d'organisation"),
        ]
    );
    assert!(
        definition
            .qualification_agents
            .iter()
            .all(|agent| agent.provider_id.is_none())
    );
    assert_eq!(definition.aggregator.provider_id, None);
    assert!(!definition.aggregator.system_prompt.is_empty());
    assert_eq!(
        qualification_agent(&profile, "maturite-technique"),
        [
            ("legacy_auto_heberge".to_owned(), 1.0),
            ("saas".to_owned(), 0.7),
            ("inconnu".to_owned(), 0.3),
        ]
    );
    let kinds: Vec<_> = qualification_agent(&profile, "type-organisation")
        .into_iter()
        .map(|(value, _)| value)
        .collect();
    assert_eq!(kinds, ["public", "grande_entreprise", "pme", "inconnu"]);
    assert_eq!(definition.verdict.reliable_from, 70);
}

#[test]
fn upgraded_iam_fixtures_get_the_default_qualification_agent() {
    for raw in [IAM_V1, IAM_V2] {
        let profile = parse(raw);
        let definition = profile.definition();
        assert_eq!(definition.qualification_agents.len(), 1);
        assert_eq!(definition.qualification_agents[0].id.as_str(), "pertinence");
        assert!(
            definition.qualification_agents[0]
                .system_prompt
                .contains(&definition.llm.relevance_criteria)
        );
        assert_eq!(
            qualification_agent(&profile, "pertinence"),
            [
                ("forte".to_owned(), 1.0),
                ("moyenne".to_owned(), 0.5),
                ("faible".to_owned(), 0.0),
            ]
        );
        assert_eq!(definition.aggregator.provider_id, None);
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_three_form() {
    let mut value: serde_json::Value = serde_json::from_str(IAM).expect("json");
    value["schema_version"] = 3.into();
    for key in ["qualification_agents", "aggregator", "verdict", "evidence"] {
        value.as_object_mut().expect("object").remove(key);
    }
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded.definition().schema_version, 6);
    assert_eq!(upgraded.definition().agents, parse(IAM).definition().agents);
    assert_eq!(upgraded.definition().qualification_agents.len(), 1);
}

#[test]
fn the_current_example_upgrades_from_its_version_four_form() {
    let mut value: serde_json::Value = serde_json::from_str(IAM).expect("json");
    value["schema_version"] = 4.into();
    value.as_object_mut().expect("object").remove("evidence");
    for agent in value["qualification_agents"].as_array_mut().expect("array") {
        agent.as_object_mut().expect("object").remove("description");
    }
    let upgraded = parse(&value.to_string());
    let current = parse(IAM);
    assert_eq!(upgraded.definition().schema_version, 6);
    for (old, new) in upgraded
        .definition()
        .qualification_agents
        .iter()
        .zip(&current.definition().qualification_agents)
    {
        assert_eq!(old.description, format!("Évalue : {}", old.name));
        assert_eq!(old.questions, new.questions);
        assert_eq!(old.system_prompt, new.system_prompt);
        assert!(!new.description.is_empty());
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_five_form() {
    let mut value: serde_json::Value = serde_json::from_str(IAM).expect("json");
    value["schema_version"] = 5.into();
    value.as_object_mut().expect("object").remove("evidence");
    value["verdict"] = serde_json::json!({"reliable_from": 70, "discard_below": 30});
    let upgraded = parse(&value.to_string());
    let current = parse(IAM);
    assert_eq!(upgraded.definition().schema_version, 6);
    assert_eq!(upgraded, current);
}

#[test]
fn contract_schema_follows_the_example_questions() {
    let profile = parse(IAM);
    let agent = &profile.definition().qualification_agents[2];
    let schema = lodestone_core::contract_schema(agent);
    assert_eq!(
        schema["properties"]["answers"]["properties"]["type"]["properties"]["choice"]["enum"],
        serde_json::json!(["public", "grande_entreprise", "pme", "inconnu"])
    );
}

#[test]
fn iam_example_max_score_and_threshold_points() {
    let profile = parse(IAM);
    assert_eq!(profile.definition().max_score(), 47.0);
    assert!((profile.definition().qualification_points() - 18.8).abs() < 1e-9);
}

#[test]
fn committed_schema_matches_definition() {
    let mut generated =
        serde_json::to_string_pretty(&schema_for!(ProfileDefinition)).expect("schema serializes");
    generated.push('\n');
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas/profile.v6.schema.json");
    if std::env::var("UPDATE_SCHEMA").as_deref() == Ok("1") {
        fs::write(&path, &generated).expect("schema file is writable");
        return;
    }
    let committed =
        fs::read_to_string(&path).expect("schema file exists, regenerate with UPDATE_SCHEMA=1");
    assert!(
        committed == generated,
        "docs/schemas/profile.v6.schema.json is stale, regenerate with UPDATE_SCHEMA=1"
    );
}
