#![allow(dead_code)]

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    CreateProfileCommand, Lead, LeadId, LodestoneUseCase, NormalizedDomain, ProfileId, ProfileName,
    ValidatedProfile,
    infrastructure::{lead::postgres::PgLeadRepository, postgres::with_tx},
};
use sqlx::PgPool;

pub const MINIMAL_PROFILE: &str = r#"{
    "schema_version": 1,
    "identity": {"offer": "Managed IAM", "target_customer": "Companies running Keycloak"},
    "sources": {
        "http_probe": {
            "fingerprints": [{
                "id": "keycloak-master-realm",
                "path": "/realms/master",
                "product": "keycloak",
                "matchers": [
                    {"kind": "status", "equals": 200},
                    {"kind": "body", "pattern": "\"realm\"\\s*:\\s*\"master\""}
                ],
                "version": {"from": "header", "name": "x-keycloak-version", "pattern": "^(\\d+\\.\\d+)"},
                "debt_markers": [{"id": "old", "rule": {"kind": "version_below", "version": "22.0"}}]
            }],
            "excluded_products": ["ferriskey"]
        }
    },
    "scoring": {"factors": [{"signal": "product_detected", "value": "keycloak", "weight": 10.0}]},
    "contact_ranking": {"preferred_local_parts": ["security", "contact"]},
    "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Self-hosts an IAM product"}
}"#;

pub async fn create_profile(pool: &PgPool, name: &str) -> ProfileId {
    let value: serde_json::Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    let command = CreateProfileCommand {
        name: ProfileName::parse(name).unwrap(),
        definition: ValidatedProfile::parse(value).unwrap(),
        created_by: None,
    };
    LodestoneUseCase::new(pool.clone())
        .create_profile(command)
        .await
        .unwrap()
        .profile
        .id
}

pub fn domain(host: &str) -> NormalizedDomain {
    NormalizedDomain::from_host(host).unwrap()
}

pub fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

pub async fn seen_lead(pool: &PgPool, profile: ProfileId, host: &str) -> Lead {
    with_tx(pool, async |shared| {
        use lodestone_core::LeadRepository;
        PgLeadRepository::new(&shared)
            .record_seen(profile, &domain(host), at(0))
            .await
    })
    .await
    .unwrap()
}

pub async fn lead_id(pool: &PgPool, profile: ProfileId, host: &str) -> LeadId {
    seen_lead(pool, profile, host).await.id
}

pub fn scoring_answer(schema: &serde_json::Value, quote: &str, worst: bool) -> serde_json::Value {
    use serde_json::{Map, Value, json};
    let mut criteria = Map::new();
    let listed = schema["properties"]["criteria"]["properties"]
        .as_object()
        .unwrap();
    for (id, criterion) in listed {
        let all = criterion["properties"]["choice"]["enum"]
            .as_array()
            .unwrap();
        let known: Vec<&Value> = all
            .iter()
            .filter(|value| value.as_str() != Some("inconnu"))
            .collect();
        let chosen = if worst {
            known[known.len() - 1]
        } else {
            known[0]
        };
        let probabilities: Map<String, Value> = all
            .iter()
            .map(|value| {
                (
                    value.as_str().unwrap().to_owned(),
                    json!(if value == chosen { 1.0 } else { 0.0 }),
                )
            })
            .collect();
        criteria.insert(
            id.clone(),
            json!({
                "type": "choice",
                "choice": chosen,
                "evidence": quote,
                "confidence": 0.9,
                "probabilities": probabilities
            }),
        );
    }
    let (score, fiable, a_verifier, ecarte) = if worst {
        (10, 0.05, 0.15, 0.8)
    } else {
        (85, 0.8, 0.15, 0.05)
    };
    json!({
        "criteria": criteria,
        "score": score,
        "verdict": {
            "choice": if worst { "ecarte" } else { "fiable" },
            "confidence": if worst { ecarte } else { fiable },
            "probabilities": {"fiable": fiable, "a_verifier": a_verifier, "ecarte": ecarte}
        },
        "reasons": ["Le lead correspond à la cible.", "Les preuves sont cohérentes."]
    })
}
