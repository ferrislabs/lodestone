use std::{fs, path::PathBuf};

use lodestone_core::{ValidatedProfile, definition::ProfileDefinition};
use schemars::schema_for;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const ECOMMERCE: &str =
    include_str!("../../../docs/examples/profiles/ecommerce-replatforming.json");

fn parse(raw: &str) -> ValidatedProfile {
    let value: serde_json::Value = serde_json::from_str(raw).expect("example is valid JSON");
    match ValidatedProfile::parse(value) {
        Ok(profile) => profile,
        Err(error) => panic!("{}: {:?}", error.path, error.problem),
    }
}

#[test]
fn iam_example_is_valid_and_has_expected_content() {
    let profile = parse(IAM);
    let sources = &profile.definition().sources;
    let labels = &sources
        .certificate_transparency
        .as_ref()
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
    let probe = sources
        .http_probe
        .as_ref()
        .expect("iam example has a probe");
    assert_eq!(probe.excluded_products, ["ferriskey"]);
    assert_eq!(probe.fingerprints.len(), 5);
}

#[test]
fn ecommerce_example_is_valid_without_iam_fields() {
    let profile = parse(ECOMMERCE);
    let sources = &profile.definition().sources;
    assert!(sources.certificate_transparency.is_none());
    assert!(sources.dns_cname.is_none());
}

#[test]
fn committed_schema_matches_definition() {
    let mut generated =
        serde_json::to_string_pretty(&schema_for!(ProfileDefinition)).expect("schema serializes");
    generated.push('\n');
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas/profile.v1.schema.json");
    if std::env::var("UPDATE_SCHEMA").as_deref() == Ok("1") {
        fs::write(&path, &generated).expect("schema file is writable");
        return;
    }
    let committed =
        fs::read_to_string(&path).expect("schema file exists, regenerate with UPDATE_SCHEMA=1");
    assert!(
        committed == generated,
        "docs/schemas/profile.v1.schema.json is stale, regenerate with UPDATE_SCHEMA=1"
    );
}
