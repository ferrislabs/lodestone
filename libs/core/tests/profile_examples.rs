use std::{fs, path::PathBuf};

use lodestone_core::{
    SourceName, ValidatedProfile,
    definition::{Agent, Escalation, ProfileDefinition},
};
use schemars::schema_for;
use serde_json::{Map, Value, from_str, json, to_string_pretty};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const IAM_V1: &str = include_str!("fixtures/profiles/iam-ferriskey.v1.json");
const IAM_V2: &str = include_str!("fixtures/profiles/iam-ferriskey.v2.json");
const IAM_V16: &str = include_str!("fixtures/profiles/iam-ferriskey.v16.json");
const IAM_V17: &str = include_str!("fixtures/profiles/iam-ferriskey.v17.json");
const IAM_V18: &str = include_str!("fixtures/profiles/iam-ferriskey.v18.json");
const ECOMMERCE: &str =
    include_str!("../../../docs/examples/profiles/ecommerce-replatforming.json");

fn parse(raw: &str) -> ValidatedProfile {
    let value: Value = from_str(raw).expect("example is valid JSON");
    match ValidatedProfile::parse(value) {
        Ok(profile) => profile,
        Err(error) => panic!("{}: {:?}", error.path, error.problem),
    }
}

fn legacy_scoring() -> Value {
    json!({"factors": [{"signal": "job_offer", "weight": 8}]})
}

const IAM_CRITERIA: &str = "The organization operates its own identity infrastructure, such as a self-hosted Keycloak, or pays for a SaaS IAM, and shows signs of migration, staffing or procurement around it. Physical access control and unrelated directories are not relevant.";

const PUBLIC_PROCUREMENT: [(&str, &str, &str); 3] = [
    ("boamp", "BOAMP", "boamp"),
    ("decp", "DECP", "decp"),
    ("ted", "TED", "ted"),
];

fn split_v16() -> Value {
    let mut value: Value = from_str(IAM_V16).expect("json");
    let agents = value["agents"].as_array_mut().expect("agents");
    let position = agents
        .iter()
        .position(|agent| agent["id"] == "marches-publics")
        .expect("iam v16 fixture has the public procurement agent");
    let merged = agents.remove(position);
    for (offset, (id, name, provider)) in PUBLIC_PROCUREMENT.iter().enumerate() {
        let mut agent = merged.clone();
        agent["id"] = (*id).into();
        agent["name"] = (*name).into();
        agent["providers"] = json!([provider]);
        agents.insert(position + offset, agent);
    }
    value
}

fn split_example() -> ValidatedProfile {
    parse(&split_v16().to_string())
}

fn version_fifteen_agent(agent: &Value) -> Value {
    let mut agent = agent.clone();
    let object = agent.as_object_mut().expect("agent object");
    let kind = object.remove("type").expect("agent type");
    if kind == "signal" {
        let source = object
            .get_mut("source")
            .and_then(Value::as_object_mut)
            .expect("signal source");
        let provider = source.remove("provider").expect("provider");
        source.insert("kind".to_owned(), provider);
    } else {
        let provider = object.remove("providers").expect("providers")[0].clone();
        let mut source = Map::new();
        source.insert("kind".to_owned(), provider);
        for key in ["keywords", "exclusions", "accept_min_keywords"] {
            if let Some(value) = object.remove(key) {
                source.insert(key.to_owned(), value);
            }
        }
        object.remove("filters");
        object.insert("source".to_owned(), Value::Object(source));
    }
    agent
}

fn version_fifteen_agents() -> Value {
    let agents = split_v16()["agents"].clone();
    Value::Array(
        agents
            .as_array()
            .expect("agents array")
            .iter()
            .map(version_fifteen_agent)
            .collect(),
    )
}

fn version_thirteen() -> Value {
    let mut value: Value = from_str(IAM_V16).expect("json");
    value["agents"] = version_fifteen_agents();
    let object = value.as_object_mut().expect("object");
    object.remove("language");
    object.insert("schema_version".to_owned(), 13.into());
    object.insert(
        "llm".to_owned(),
        json!({
            "qualification_threshold": 40,
            "relevance_criteria": IAM_CRITERIA,
            "language": "fr"
        }),
    );
    object.insert("calibration".to_owned(), json!({"learned": true}));
    value
}

fn agent_ids(profile: &ValidatedProfile) -> Vec<&str> {
    profile
        .definition()
        .agents
        .iter()
        .map(|agent| agent.id().as_str())
        .collect()
}

#[test]
fn iam_example_is_valid_and_has_expected_content() {
    let profile = parse(IAM);
    let definition = profile.definition();
    assert_eq!(
        definition
            .agents
            .iter()
            .map(|agent| (agent.id().as_str(), agent.name()))
            .collect::<Vec<_>>(),
        [
            ("certificats", "Certificats"),
            ("dns", "DNS"),
            ("marches-publics", "Marchés publics"),
            ("offres-et-missions", "Offres et missions"),
        ]
    );
    let labels = &definition
        .enabled_agents(SourceName::CertificateTransparency)
        .find_map(Agent::certificates)
        .expect("iam example has a certificates agent")
        .labels;
    for expected in [
        "auth", "sso", "login", "iam", "keycloak", "id", "accounts", "identity", "idp",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected}"
        );
    }
    assert!(definition.inspection.enabled);
    assert!(definition.probes_hosts());
    let probe = &definition.enrichment.custom;
    assert_eq!(probe.excluded_products, ["ferriskey"]);
    assert_eq!(probe.fingerprints.len(), 5);
    assert!(
        probe
            .fingerprints
            .iter()
            .any(|fingerprint| fingerprint.id == "keycloak-realm")
    );
    assert_eq!(
        probe.extra_subdomains,
        ["auth", "sso", "login", "id", "idp", "accounts"]
    );
}

#[test]
fn iam_example_reads_the_job_offers_of_every_lead() {
    let profile = parse(IAM);
    let definition = profile.definition();
    let jobs = &definition.enrichment.highlight_keywords;
    assert!(definition.reads_company_jobs());
    assert_eq!(jobs[0], "Keycloak");
    assert!(jobs.iter().any(|keyword| keyword == "single sign-on"));
    assert_eq!(definition.enabled_search_agents(SourceName::Ats).count(), 0);
    assert_eq!(definition.targets(SourceName::Ats).len(), 1);
    assert!(
        definition.qualification_agents[0]
            .system_prompt
            .contains("Criteria for job offers")
    );
    let offers = definition
        .agents
        .iter()
        .filter_map(Agent::as_search)
        .find(|agent| agent.id.as_str() == "offres-et-missions")
        .expect("offers agent");
    assert_eq!(offers.providers, ["france_travail", "hacker_news_hiring"]);
}

#[test]
fn iam_example_has_one_public_procurement_agent_over_three_providers() {
    let profile = parse(IAM);
    let agent = profile
        .definition()
        .agents
        .iter()
        .find(|agent| agent.id().as_str() == "marches-publics")
        .and_then(Agent::as_search)
        .expect("public procurement search agent");
    assert_eq!(agent.name, "Marchés publics");
    assert_eq!(agent.providers, ["boamp", "decp", "ted"]);
    assert!(agent.filters.is_open());
    assert_eq!(agent.keywords.len(), 8);
    assert_eq!(agent.exclusions.len(), 3);
    for source in [SourceName::Boamp, SourceName::Decp, SourceName::Ted] {
        assert_eq!(
            profile
                .definition()
                .enabled_search_agents(source)
                .map(|agent| agent.id.as_str())
                .collect::<Vec<_>>(),
            ["marches-publics"]
        );
    }
    assert_eq!(
        profile
            .definition()
            .enabled_search_agents(SourceName::FranceTravail)
            .count(),
        1
    );
}

fn assert_upgraded_iam_agents(upgraded: &ValidatedProfile) {
    assert_eq!(
        agent_ids(upgraded),
        [
            "certificate-transparency",
            "dns-cname",
            "marches-publics",
            "france-travail"
        ]
    );
    let json = upgraded.to_json();
    assert_eq!(json["agents"][0]["type"], "certificates");
    assert_eq!(json["agents"][1]["type"], "dns");
    assert_eq!(json["agents"][2]["name"], "Marchés publics");
    assert_eq!(
        json["agents"][2]["providers"],
        json!(["boamp", "decp", "ted"])
    );
    assert_eq!(json["agents"][3]["providers"], json!(["france_travail"]));
    let enrichment = &upgraded.definition().enrichment;
    assert_eq!(enrichment.custom.fingerprints.len(), 5);
    assert_eq!(enrichment.custom.excluded_products, ["ferriskey"]);
    assert_eq!(
        enrichment.custom.extra_subdomains,
        ["auth", "sso", "login", "id", "idp", "accounts"]
    );
    assert!(!enrichment.highlight_keywords.is_empty());
    assert!(upgraded.definition().inspection.enabled);
}

#[test]
fn the_version_sixteen_iam_example_upgrades_to_sourcing_and_enrichment() {
    let upgraded = parse(IAM_V16);
    assert_eq!(upgraded.definition().schema_version, 19);
    assert_upgraded_iam_agents(&upgraded);
    let jobs = &upgraded.definition().enrichment.highlight_keywords;
    assert_eq!(
        *jobs,
        [
            "Keycloak",
            "IAM",
            "ingénieur IAM",
            "migration Okta",
            "migration Auth0",
            "gestion des identités",
            "identity and access management",
            "SSO",
            "single sign-on",
            "Okta migration"
        ]
    );
    let current = parse(IAM);
    assert_eq!(
        upgraded.definition().enrichment.custom,
        current.definition().enrichment.custom
    );
    assert_eq!(jobs, &current.definition().enrichment.highlight_keywords);
}

#[test]
fn the_version_seventeen_iam_example_upgrades_to_the_current_example() {
    let raw: Value = from_str(IAM_V17).expect("json");
    assert_eq!(raw["schema_version"], 17);
    assert_eq!(raw["enrichment"]["http_probe"]["enabled"], true);
    let upgraded = parse(IAM_V17);
    assert_eq!(upgraded, parse(IAM_V18));
    let json = upgraded.to_json();
    assert_eq!(json["schema_version"], 19);
    assert_eq!(json["inspection"], json!({"enabled": true}));
    assert!(json["enrichment"].get("http_probe").is_none());
    assert!(json["enrichment"].get("company_jobs").is_none());
}

#[test]
fn a_version_eighteen_profile_gains_empty_countries_on_its_host_agents() {
    let raw: Value = from_str(IAM_V18).expect("json");
    assert_eq!(raw["schema_version"], 18);
    let upgraded = parse(IAM_V18).to_json();
    assert_eq!(upgraded["schema_version"], 19);
    assert_eq!(upgraded["agents"][0]["countries"], json!([]));
    assert_eq!(upgraded["agents"][1]["countries"], json!([]));
    let mut expected: Value = from_str(IAM).expect("json");
    for index in [0, 1] {
        expected["agents"][index]["countries"] = json!([]);
    }
    assert_eq!(upgraded, parse(&expected.to_string()).to_json());
}

#[test]
fn the_iam_example_targets_france_on_its_host_agents() {
    let profile = parse(IAM);
    let countries: Vec<Vec<&str>> = profile
        .definition()
        .agents
        .iter()
        .filter_map(|agent| agent.host_countries())
        .map(|countries| countries.iter().map(|code| code.as_str()).collect())
        .collect();
    assert_eq!(countries, [vec!["FR"], vec!["FR"]]);
    let mut invalid: Value = from_str(IAM).expect("json");
    invalid["agents"][1]["countries"] = json!(["fr"]);
    let error = ValidatedProfile::parse(invalid).expect_err("lowercase code");
    assert_eq!(error.path, "agents[1].countries[0]");
}

#[test]
fn a_version_seventeen_profile_without_identity_fingerprints_loses_the_iam_default_subdomains() {
    let mut value: Value = from_str(ECOMMERCE).expect("json");
    let custom = value["enrichment"]["custom"].clone();
    value["schema_version"] = 17.into();
    value.as_object_mut().expect("object").remove("inspection");
    value["enrichment"] = json!({
        "http_probe": {
            "enabled": true,
            "fingerprints": custom["fingerprints"],
            "excluded_products": [],
            "extra_subdomains": ["auth", "sso", "login", "id", "idp", "accounts"]
        },
        "company_jobs": {"enabled": false, "keywords": ["Magento"], "accept_min_keywords": 2}
    });
    let upgraded = parse(&value.to_string());
    let enrichment = &upgraded.definition().enrichment;
    assert!(enrichment.custom.extra_subdomains.is_empty());
    assert_eq!(enrichment.custom.fingerprints.len(), 1);
    assert!(enrichment.highlight_keywords.is_empty());
}

#[test]
fn version_fifteen_agents_merge_and_leave_probes_and_job_pages_to_the_enrichment() {
    let mut value: Value = from_str(IAM_V16).expect("json");
    value["schema_version"] = 15.into();
    value["agents"] = version_fifteen_agents();
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    assert_eq!(upgraded, parse(IAM_V16));
    assert_upgraded_iam_agents(&upgraded);
    let json = upgraded.to_json();
    assert_eq!(json["agents"][2]["filters"]["remote"], "any");
    assert_eq!(json["agents"][2]["filters"]["contracts"], json!([]));
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

fn parse_value(value: Value) -> ValidatedProfile {
    ValidatedProfile::parse(value).expect("round trip parses")
}

#[test]
fn version_one_iam_fixture_is_upgraded_to_the_current_example() {
    let upgraded = parse(IAM_V1);
    let current = split_example();
    assert_eq!(upgraded.definition().schema_version, 19);
    assert_upgraded_iam_agents(&upgraded);
    let names: Vec<_> = upgraded
        .definition()
        .agents
        .iter()
        .map(|agent| agent.name())
        .collect();
    assert_eq!(
        names,
        [
            "Certificate transparency",
            "DNS CNAME",
            "Marchés publics",
            "France Travail"
        ]
    );
    assert!(
        upgraded
            .definition()
            .agents
            .iter()
            .all(|agent| agent.enabled())
    );
    let strip = |profile: &ValidatedProfile| {
        let mut value = profile.to_json();
        value["qualification_agents"] = Value::Null;
        value["aggregator"] = Value::Null;
        value["enrichment"]["highlight_keywords"] = Value::Null;
        value
    };
    assert_eq!(strip(&upgraded), strip(&current));
    assert_eq!(
        upgraded.definition().enrichment.highlight_keywords,
        [
            "Keycloak",
            "IAM",
            "ingénieur IAM",
            "migration Okta",
            "migration Auth0",
            "gestion des identités"
        ]
    );
    assert_eq!(
        upgraded.definition().language,
        current.definition().language
    );
}

#[test]
fn version_two_iam_fixture_is_upgraded_to_version_six() {
    let upgraded = parse(IAM_V2);
    let definition = upgraded.definition();
    assert_eq!(definition.schema_version, 19);
    assert_eq!(upgraded.to_json()["schema_version"], 19);
    assert!(upgraded.to_json().get("scoring").is_none());
    assert!(upgraded.to_json().get("llm").is_none());
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
fn iam_example_has_eight_qualification_agents() {
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
            ("environnement", "Environnement"),
            (
                "type-et-taille-d-organisation",
                "Type et taille d'organisation"
            ),
            ("marche", "Marché"),
            ("opportunite", "Opportunité"),
            ("recherche-entreprise", "Recherche entreprise"),
            ("exclusions", "Exclusions"),
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
    let kinds: Vec<_> = qualification_agent(&profile, "type-et-taille-d-organisation")
        .into_iter()
        .map(|(value, _)| value)
        .collect();
    assert_eq!(
        kinds,
        [
            "secteur_public",
            "grand_compte",
            "eti",
            "pme",
            "startup",
            "inconnu"
        ]
    );
    for id in ["environnement", "marche", "opportunite"] {
        let agent = definition
            .qualification_agents
            .iter()
            .find(|agent| agent.id.as_str() == id)
            .expect("agent exists");
        assert_eq!(agent.questions.len(), 2, "{id}");
        assert!(agent.enabled, "{id}");
    }
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
                .contains("Critères de pertinence : ")
        );
        assert_eq!(
            qualification_agent(&profile, "pertinence"),
            [
                ("forte".to_owned(), 1.0),
                ("moyenne".to_owned(), 0.5),
                ("faible".to_owned(), 0.0),
                ("inconnu".to_owned(), 0.5),
            ]
        );
        assert_eq!(definition.aggregator.provider_id, None);
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_three_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 3.into();
    value["scoring"] = legacy_scoring();
    for key in [
        "qualification_agents",
        "aggregator",
        "verdict",
        "evidence",
        "escalation",
        "scoring_agents",
        "calibration",
    ] {
        value.as_object_mut().expect("object").remove(key);
    }
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded.definition().schema_version, 19);
    assert_eq!(
        upgraded.definition().agents,
        split_example().definition().agents
    );
    assert_eq!(upgraded.definition().qualification_agents.len(), 1);
}

fn without_exclusions_agent(value: &mut Value) {
    value["qualification_agents"]
        .as_array_mut()
        .expect("array")
        .retain(|agent| agent["id"] != "exclusions");
}

fn without_sampling_fields(value: &mut Value) {
    value.as_object_mut().expect("object").remove("escalation");
    value
        .as_object_mut()
        .expect("object")
        .remove("scoring_agents");
    value.as_object_mut().expect("object").remove("calibration");
    for agent in value["qualification_agents"].as_array_mut().expect("array") {
        let agent = agent.as_object_mut().expect("object");
        agent.remove("samples");
        agent.remove("sampling_temperature");
    }
}

fn without_exclusion_fields(value: &mut Value) {
    without_exclusions_agent(value);
    without_sampling_fields(value);
    for agent in value["qualification_agents"].as_array_mut().expect("array") {
        agent
            .as_object_mut()
            .expect("object")
            .remove("exclusion_threshold");
        for question in agent["questions"].as_array_mut().expect("array") {
            for option in question["options"].as_array_mut().expect("array") {
                option.as_object_mut().expect("object").remove("excludes");
            }
        }
    }
}

fn without_unknown_flags(value: &mut Value) {
    without_exclusion_fields(value);
    for agent in value["qualification_agents"].as_array_mut().expect("array") {
        for question in agent["questions"].as_array_mut().expect("array") {
            for option in question["options"].as_array_mut().expect("array") {
                option.as_object_mut().expect("object").remove("is_unknown");
            }
        }
    }
}

fn current_without_exclusions_agent() -> ValidatedProfile {
    let mut value = version_thirteen();
    without_exclusions_agent(&mut value);
    parse(&value.to_string())
}

#[test]
fn the_current_example_upgrades_from_its_version_nine_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 9.into();
    value["scoring"] = legacy_scoring();
    value
        .as_object_mut()
        .expect("object")
        .remove("scoring_agents");
    value.as_object_mut().expect("object").remove("calibration");
    value["evidence"]
        .as_object_mut()
        .expect("evidence section")
        .remove("half_life_days");
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    let evidence = &upgraded.definition().evidence;
    assert_eq!(upgraded.definition().schema_version, 19);
    assert_eq!(evidence.half_life_days_for("job_offer"), 90);
    assert_eq!(evidence.half_life_days_for("public_tender"), 180);
    assert_eq!(evidence.half_life_days_for("saas_provider"), 365);
}

#[test]
fn the_current_example_upgrades_from_its_version_eleven_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 11.into();
    value["scoring"] = legacy_scoring();
    value.as_object_mut().expect("object").remove("calibration");
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    assert_eq!(upgraded.definition().schema_version, 19);
    assert!(upgraded.to_json().get("calibration").is_none());
    assert!(
        upgraded
            .definition()
            .scoring_agents
            .iter()
            .all(|agent| agent.learned_calibration)
    );
}

#[test]
fn version_fourteen_drops_the_qualification_agent_weights() {
    let mut value: Value = from_str(IAM_V16).expect("json");
    value["schema_version"] = 14.into();
    value["agents"] = version_fifteen_agents();
    let agents = value["qualification_agents"]
        .as_array_mut()
        .expect("agents");
    for (index, agent) in agents.iter_mut().enumerate() {
        agent["weight"] = (index as u64 % 10 + 1).into();
    }
    let scoring_weight = value["scoring_agents"][0]["weight"].clone();
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    assert_eq!(upgraded.definition().schema_version, 19);
    let json = upgraded.to_json();
    for agent in json["qualification_agents"].as_array().expect("agents") {
        assert!(agent.get("weight").is_none());
        assert!(agent["questions"][0].get("weight").is_some());
    }
    assert_eq!(json["scoring_agents"][0]["weight"], scoring_weight);
}

#[test]
fn version_thirteen_weights_are_dropped_too() {
    let mut value = version_thirteen();
    value["qualification_agents"][0]["weight"] = 9.into();
    let upgraded = parse(&value.to_string());
    assert!(
        upgraded.to_json()["qualification_agents"][0]
            .get("weight")
            .is_none()
    );
}

#[test]
fn the_current_example_upgrades_from_its_version_twelve_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 12.into();
    value["scoring"] = legacy_scoring();
    value["evidence"]["reference_score"] = 50.into();
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    assert_eq!(upgraded.definition().schema_version, 19);
    let json = upgraded.to_json();
    assert!(json.get("scoring").is_none());
    assert!(json["evidence"].get("reference_score").is_none());
}

#[test]
fn the_examples_carry_neither_signal_weights_nor_a_reference_score() {
    for raw in [IAM, ECOMMERCE] {
        let value: Value = from_str(raw).expect("json");
        assert!(value.get("scoring").is_none());
        assert!(value["evidence"].get("reference_score").is_none());
    }
}

#[test]
fn the_examples_learn_the_calibration_of_every_agent() {
    for raw in [IAM, ECOMMERCE] {
        let profile = parse(raw);
        let definition = profile.definition();
        assert!(
            definition
                .qualification_agents
                .iter()
                .all(|agent| agent.learned_calibration)
        );
        assert!(
            definition
                .scoring_agents
                .iter()
                .all(|agent| agent.learned_calibration)
        );
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_thirteen_form() {
    let mut value = version_thirteen();
    value["agents"][8]["llm"] = json!({"relevance_criteria": "Offers for identity engineers"});
    value["calibration"] = json!({"learned": false});
    let upgraded = parse(&value.to_string());
    let definition = upgraded.definition();
    assert_eq!(definition.schema_version, 19);
    assert!(
        definition.qualification_agents[0].system_prompt.ends_with(
            "Critères de l'agent « Job offers (English) » : Offers for identity engineers"
        )
    );
    assert!(
        definition
            .qualification_agents
            .iter()
            .all(|agent| !agent.learned_calibration)
    );
    assert!(
        definition
            .scoring_agents
            .iter()
            .all(|agent| !agent.learned_calibration)
    );
    let json = upgraded.to_json();
    assert!(json.get("llm").is_none());
    assert!(json.get("calibration").is_none());
    assert!(
        json["agents"]
            .as_array()
            .expect("agents")
            .iter()
            .all(|agent| agent.get("llm").is_none())
    );
}

#[test]
fn the_current_example_upgrades_from_its_version_ten_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 10.into();
    value["scoring"] = legacy_scoring();
    value
        .as_object_mut()
        .expect("object")
        .remove("scoring_agents");
    value.as_object_mut().expect("object").remove("calibration");
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    let definition = upgraded.definition();
    assert_eq!(definition.schema_version, 19);
    assert_eq!(definition.scoring_agents.len(), 1);
    let agent = &definition.scoring_agents[0];
    assert_eq!(agent.id.as_str(), "score-global");
    assert_eq!(agent.name, "Score global");
    assert_eq!(agent.provider_id, None);
    assert_eq!(agent.weight, 5);
    assert_eq!(agent.samples, 1);
}

#[test]
fn the_examples_carry_the_default_scoring_agent() {
    for raw in [IAM, ECOMMERCE] {
        let profile = parse(raw);
        let agents = &profile.definition().scoring_agents;
        assert_eq!(agents.len(), 1);
        assert_eq!(
            agents[0]
                .criteria
                .iter()
                .map(|criterion| criterion.id.as_str())
                .collect::<Vec<_>>(),
            [
                "adequation_offre",
                "solidite_preuves",
                "priorite_commerciale"
            ]
        );
        let schema = lodestone_core::scoring_contract_schema(&agents[0]);
        assert_eq!(schema["properties"]["score"]["maximum"], 100);
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_eight_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 8.into();
    value["scoring"] = legacy_scoring();
    without_sampling_fields(&mut value);
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, split_example());
    let definition = upgraded.definition();
    assert_eq!(definition.schema_version, 19);
    assert_eq!(definition.escalation, Escalation::default());
    assert_eq!(definition.escalation.provider, None);
    assert_eq!(
        (
            definition.escalation.band_low,
            definition.escalation.band_high
        ),
        (40, 70)
    );
    for agent in &definition.qualification_agents {
        assert_eq!(agent.samples, 3);
        assert_eq!(agent.sampling_temperature, 0.7);
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_seven_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 7.into();
    value["scoring"] = legacy_scoring();
    without_exclusion_fields(&mut value);
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, current_without_exclusions_agent());
    for agent in &upgraded.definition().qualification_agents {
        assert_eq!(agent.exclusion_threshold, 0.6);
        assert!(
            agent
                .questions
                .iter()
                .flat_map(|question| &question.options)
                .all(|option| !option.excludes)
        );
    }
}

#[test]
fn the_iam_example_has_an_enabled_exclusions_agent() {
    let profile = parse(IAM);
    let agent = profile
        .definition()
        .qualification_agents
        .iter()
        .find(|agent| agent.id.as_str() == "exclusions")
        .expect("exclusions agent");
    assert!(agent.enabled);
    assert_eq!(agent.exclusion_threshold, 0.6);
    assert_eq!(
        agent
            .questions
            .iter()
            .map(|question| question.id.as_str())
            .collect::<Vec<_>>(),
        [
            "editeur_integrateur",
            "hebergeur_agence",
            "domaine_inactif",
            "client_actuel",
            "concurrent"
        ]
    );
    for question in &agent.questions {
        let excluding: Vec<_> = question
            .options
            .iter()
            .filter(|option| option.excludes)
            .collect();
        assert_eq!(excluding.len(), 1, "{}", question.id);
        assert_eq!(excluding[0].score, 0.0);
        assert!(
            question
                .unknown_option()
                .is_some_and(|unknown| !unknown.excludes)
        );
    }
    for list in [
        "Clients actuels :",
        "Concurrents :",
        "Éditeurs et intégrateurs du domaine :",
    ] {
        assert!(agent.system_prompt.contains(list), "{list}");
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_six_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 6.into();
    value["scoring"] = legacy_scoring();
    without_unknown_flags(&mut value);
    let upgraded = parse(&value.to_string());
    assert_eq!(upgraded, current_without_exclusions_agent());
    for agent in &upgraded.definition().qualification_agents {
        for question in &agent.questions {
            assert_eq!(
                question
                    .options
                    .iter()
                    .filter(|option| option.is_unknown)
                    .count(),
                1
            );
        }
    }
}

#[test]
fn the_current_example_upgrades_from_its_version_four_form() {
    let mut value = version_thirteen();
    value["schema_version"] = 4.into();
    value["scoring"] = legacy_scoring();
    without_unknown_flags(&mut value);
    value.as_object_mut().expect("object").remove("evidence");
    for agent in value["qualification_agents"].as_array_mut().expect("array") {
        agent.as_object_mut().expect("object").remove("description");
    }
    let upgraded = parse(&value.to_string());
    let current = current_without_exclusions_agent();
    assert_eq!(upgraded.definition().schema_version, 19);
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
    let mut value = version_thirteen();
    value["schema_version"] = 5.into();
    value["scoring"] = legacy_scoring();
    without_unknown_flags(&mut value);
    value.as_object_mut().expect("object").remove("evidence");
    value["verdict"] = json!({"reliable_from": 70, "discard_below": 30});
    let upgraded = parse(&value.to_string());
    let current = current_without_exclusions_agent();
    assert_eq!(upgraded.definition().schema_version, 19);
    assert_eq!(upgraded, current);
}

#[test]
fn contract_schema_follows_the_example_questions() {
    let profile = parse(IAM);
    let agent = &profile.definition().qualification_agents[2];
    assert_eq!(agent.id.as_str(), "environnement");
    let schema = lodestone_core::contract_schema(agent);
    assert_eq!(
        schema["properties"]["answers"]["properties"]["environnement"]["properties"]["choice"]["enum"],
        json!(["production", "mixte", "hors_production", "inconnu"])
    );
}

#[test]
fn committed_schema_matches_definition() {
    let mut generated =
        to_string_pretty(&schema_for!(ProfileDefinition)).expect("schema serializes");
    generated.push('\n');
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schemas/profile.v19.schema.json");
    if std::env::var("UPDATE_SCHEMA").as_deref() == Ok("1") {
        fs::write(&path, &generated).expect("schema file is writable");
        return;
    }
    let committed =
        fs::read_to_string(&path).expect("schema file exists, regenerate with UPDATE_SCHEMA=1");
    assert!(
        committed == generated,
        "docs/schemas/profile.v19.schema.json is stale, regenerate with UPDATE_SCHEMA=1"
    );
}
