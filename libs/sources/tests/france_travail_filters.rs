use std::{num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use common::Secret;
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    ActiveProfile, Profile, ProfileId, ProfileName, ProfileRevision, RevisionNumber, SourceError,
    ValidatedProfile,
};
use serde_json::{Value, json};
use sources::france_travail::{
    AccessProvider, FranceTravailAccess, FranceTravailSettings, FranceTravailSource,
};
use url::Url;
use uuid::Uuid;

const ACCESS_TOKEN: &str = "synthetic-access-token-0001";

struct StaticAccess {
    api_base_url: Url,
}

impl AccessProvider for StaticAccess {
    async fn access(&self) -> Result<FranceTravailAccess, SourceError> {
        Ok(FranceTravailAccess {
            token: Secret::new(ACCESS_TOKEN),
            api_base_url: self.api_base_url.clone(),
        })
    }

    fn invalidate(&self) {}
}

fn legacy_value() -> Value {
    json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"france_travail": {"keywords": ["keycloak"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    })
}

fn profile_with_agents(agents: Value) -> ActiveProfile {
    let mut value = ValidatedProfile::parse(legacy_value()).unwrap().to_json();
    value["agents"] = agents;
    active(ValidatedProfile::parse(value).unwrap())
}

fn search_agent(id: &str, keywords: &[&str], filters: Value) -> Value {
    json!({
        "type": "search",
        "id": id,
        "name": id,
        "providers": ["france_travail"],
        "keywords": keywords,
        "filters": filters,
    })
}

fn polite() -> Arc<PoliteFetcher> {
    Arc::new(
        PoliteFetcher::new(FetchSettings {
            contact: "bot@example.com".to_owned(),
            requests_per_host_per_minute: NonZeroU32::new(1000).unwrap(),
            request_timeout: Duration::from_secs(5),
            max_body_bytes: 1024 * 1024,
            max_redirects: 3,
            robots_ttl: Duration::from_secs(60),
            source_quotas: Vec::new(),
            allow_private_targets: true,
        })
        .unwrap(),
    )
}

fn active(definition: ValidatedProfile) -> ActiveProfile {
    let profile_id = ProfileId(Uuid::from_u128(1));
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: profile_id,
            name: ProfileName::parse("p").unwrap(),
            active: true,
            created_at: epoch,
            updated_at: epoch,
        },
        revision: ProfileRevision {
            profile_id,
            number: RevisionNumber::new(1).unwrap(),
            definition,
            created_by: None,
            created_at: epoch,
        },
    }
}

#[tokio::test]
async fn contract_filters_become_type_contrat_and_the_rest_stays_for_post_filtering() {
    let server = MockServer::start_async().await;
    let freelance = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "keycloak")
                .query_param("typeContrat", "FRA,LIB");
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let employee = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "iam")
                .query_param("typeContrat", "CDD,CDI,MIS,SAI");
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let open = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "sso")
                .matches(|request| {
                    !request
                        .query_params
                        .as_ref()
                        .is_some_and(|pairs| pairs.iter().any(|(key, _)| key == "typeContrat"))
                });
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let remote_only = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "okta")
                .matches(|request| {
                    !request.query_params.as_ref().is_some_and(|pairs| {
                        pairs
                            .iter()
                            .any(|(key, _)| key == "typeContrat" || key == "teletravail")
                    })
                });
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let profile = profile_with_agents(json!([
        search_agent(
            "freelance",
            &["keycloak"],
            json!({"contracts": ["freelance"]})
        ),
        search_agent(
            "employee",
            &["iam"],
            json!({"contracts": ["employee", "public_contract"]})
        ),
        search_agent("open", &["sso"], json!({"contracts": ["any"]})),
        search_agent(
            "remote",
            &["okta"],
            json!({"remote": "full_remote", "countries": ["FR"]})
        ),
    ]));
    let source = FranceTravailSource::new(
        polite(),
        StaticAccess {
            api_base_url: Url::parse(&server.url("/v2")).unwrap(),
        },
        FranceTravailSettings {
            published_since_days: 7,
            max_pages: 1,
        },
    );

    let (_, stats) = source.collect_with_stats(&[profile]).await.unwrap();

    assert_eq!(stats.queries, 4);
    freelance.assert_hits_async(1).await;
    employee.assert_hits_async(1).await;
    open.assert_hits_async(1).await;
    remote_only.assert_hits_async(1).await;
}

#[tokio::test]
async fn the_same_keyword_is_queried_once_per_distinct_native_filter() {
    let server = MockServer::start_async().await;
    let plain = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "keycloak")
                .matches(|request| {
                    !request
                        .query_params
                        .as_ref()
                        .is_some_and(|pairs| pairs.iter().any(|(key, _)| key == "typeContrat"))
                });
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let filtered = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/v2/offres/search")
                .query_param("motsCles", "keycloak")
                .query_param("typeContrat", "FRA,LIB");
            then.status(200).json_body(json!({"resultats": []}));
        })
        .await;
    let profile = profile_with_agents(json!([
        search_agent("a", &["keycloak"], json!({})),
        search_agent("b", &["Keycloak"], json!({})),
        search_agent("c", &["keycloak"], json!({"contracts": ["freelance"]})),
    ]));
    let source = FranceTravailSource::new(
        polite(),
        StaticAccess {
            api_base_url: Url::parse(&server.url("/v2")).unwrap(),
        },
        FranceTravailSettings {
            published_since_days: 7,
            max_pages: 1,
        },
    );

    source.collect_with_stats(&[profile]).await.unwrap();

    plain.assert_hits_async(1).await;
    filtered.assert_hits_async(1).await;
}
