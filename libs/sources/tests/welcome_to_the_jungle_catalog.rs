use std::{collections::BTreeMap, num::NonZeroU32, sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use common::{EncryptionKey, Secret};
use fetch::{FetchSettings, PoliteFetcher};
use httpmock::{Method::GET, MockServer};
use lodestone_core::{
    ActiveProfile, LodestoneUseCase, Profile, ProfileId, ProfileName, ProfileRevision,
    ProviderStatus, RevisionNumber, SourceError, ValidatedProfile, WELCOME_TO_THE_JUNGLE,
    infrastructure::crypto::AesGcmCipher, provider_descriptor,
    source_providers::SaveProviderConfiguration,
};
use serde_json::{Map, Value, json};
use sources::welcome_to_the_jungle::{
    AccessSource, Endpoints, WelcomeToTheJungleSettings, WelcomeToTheJungleSource,
};
use sqlx::PgPool;
use url::Url;
use uuid::Uuid;

const REFERER: &str = "https://www.welcometothejungle.com/fr";

fn cipher() -> AesGcmCipher {
    let encoded = format!("{}QUE=", "QUFB".repeat(10));
    AesGcmCipher::new(&EncryptionKey::from_base64(&Secret::new(encoded)).unwrap()).unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn profile() -> ActiveProfile {
    let legacy = json!({
        "schema_version": 1,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
        "sources": {"boamp": {"keywords": ["keycloak"]}},
        "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
    });
    let mut value = ValidatedProfile::parse(legacy).unwrap().to_json();
    value["agents"] = json!([{
        "type": "search",
        "id": "wttj",
        "name": "wttj",
        "providers": ["welcome_to_the_jungle"],
        "keywords": ["keycloak"],
    }]);
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
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: epoch,
        },
    }
}

fn source(
    server: &MockServer,
    usecase: &LodestoneUseCase,
) -> WelcomeToTheJungleSource<PoliteFetcher> {
    WelcomeToTheJungleSource::new(
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
        ),
        WelcomeToTheJungleSettings {
            endpoints: Endpoints {
                site: Url::parse(&server.url("/")).unwrap(),
                search: Some(Url::parse(&server.url("/")).unwrap()),
            },
            access: AccessSource::Catalog(usecase.clone()),
            max_company_lookups: 5,
        },
    )
}

async fn configure(usecase: &LodestoneUseCase) {
    usecase
        .save_provider_configuration(
            provider_descriptor(WELCOME_TO_THE_JUNGLE).unwrap(),
            SaveProviderConfiguration {
                settings: object(json!({
                    "authorised": true,
                    "authorisation_reference": "Courriel du 9 octobre 2026",
                    "referer": REFERER,
                    "jobs_index": "jobs_test",
                    "requests_per_minute": 30,
                })),
                secrets: BTreeMap::new(),
            },
            true,
            &cipher(),
        )
        .await
        .unwrap();
    usecase
        .record_provider_status(
            WELCOME_TO_THE_JUNGLE,
            ProviderStatus::Ready,
            None,
            Utc::now(),
        )
        .await
        .unwrap();
}

async fn mock_env(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET).path("/api/env");
            then.status(200).body(
                "window.env = {\"PUBLIC_ALGOLIA_API_KEY_CLIENT\":\"search-only-key\",\"PUBLIC_ALGOLIA_APPLICATION_ID\":\"TESTAPP\"};",
            );
        })
        .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_referer_and_the_index_are_read_from_the_catalog_and_the_authorisation_is_dated(
    pool: PgPool,
) {
    let usecase = LodestoneUseCase::new(pool);
    let server = MockServer::start_async().await;
    let source = source(&server, &usecase);
    let unconfigured = source.collect_with_stats(&[profile()]).await.unwrap_err();
    assert!(
        matches!(unconfigured, SourceError::Upstream(message) if message.contains("not configured"))
    );
    configure(&usecase).await;
    mock_env(&server).await;
    let search = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/1/indexes/jobs_test")
                .query_param("query", "keycloak")
                .header("referer", REFERER);
            then.status(200)
                .json_body(json!({"hits": [], "nbPages": 1}));
        })
        .await;

    source.collect_with_stats(&[profile()]).await.unwrap();

    search.assert_hits_async(1).await;
    let stored = usecase
        .get_provider_configuration(WELCOME_TO_THE_JUNGLE)
        .await
        .unwrap()
        .unwrap();
    assert!(stored.settings["authorised_at"].as_str().is_some());
    assert_eq!(stored.status, ProviderStatus::Ready);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_refused_referer_marks_the_provider_failing(pool: PgPool) {
    let usecase = LodestoneUseCase::new(pool);
    let server = MockServer::start_async().await;
    configure(&usecase).await;
    mock_env(&server).await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/1/indexes/jobs_test");
            then.status(403).json_body(
                json!({"message": "Method not allowed with this referer", "status": 403}),
            );
        })
        .await;

    source(&server, &usecase)
        .collect_with_stats(&[profile()])
        .await
        .unwrap_err();

    let stored = usecase
        .get_provider_configuration(WELCOME_TO_THE_JUNGLE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, ProviderStatus::Failing);
    assert!(
        stored
            .last_test_message
            .is_some_and(|message| message.contains("Referer"))
    );
    assert!(
        !usecase
            .is_provider_usable(WELCOME_TO_THE_JUNGLE)
            .await
            .unwrap()
    );
}
