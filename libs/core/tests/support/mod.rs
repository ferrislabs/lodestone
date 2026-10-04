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
