mod support;

use httpmock::Method::GET;
use lodestone_core::{Credential, ProviderStatus, SourceProviderDescriptor, provider_descriptor};
use serde_json::json;
use source_providers::AuthFailure;
use sqlx::PgPool;
use support::Rig;

use lodestone_core::{
    AuthSpec, Capabilities, FieldSpec, FilterSupport, ProviderCategory, ProviderConfiguration,
};

static API_KEY_FIELDS: [FieldSpec; 1] = [FieldSpec::secret("api_key", "Clé d'API", true)];
static API_KEY_CATALOG: [SourceProviderDescriptor; 1] = [SourceProviderDescriptor {
    key: "keyed",
    name: "Keyed",
    description: "",
    category: ProviderCategory::Companies,
    auth: AuthSpec::ApiKey {
        header: "X-Api-Key",
        prefix: "Token ",
    },
    fields: &API_KEY_FIELDS,
    capabilities: Capabilities {
        keyword_search: false,
        filters: FilterSupport {
            remote: false,
            countries: false,
            contract: false,
        },
    },
    terms_note: None,
}];

#[sqlx::test(migrations = "../../migrations")]
async fn boamp_needs_no_credentials_and_is_ready_when_its_api_answers(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("boamp").unwrap();
    rig.save(
        descriptor,
        json!({"base_url": rig.server.url("/records")}),
        &[],
    )
    .await;
    let records = rig
        .server
        .mock_async(|when, then| {
            when.method(GET).path("/records").query_param("limit", "1");
            then.status(200).json_body(json!({"results": []}));
        })
        .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "boamp")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Ready);
    assert!(rig.uc.is_provider_usable("boamp").await.unwrap());
    records.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn boamp_fails_when_its_api_answers_an_error(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("boamp").unwrap();
    rig.save(
        descriptor,
        json!({"base_url": rig.server.url("/records")}),
        &[],
    )
    .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/records");
            then.status(503);
        })
        .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "boamp")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Failing);
    assert!(tested.last_test_message.unwrap().contains("503"));
    assert!(!rig.uc.is_provider_usable("boamp").await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn boamp_fails_when_its_api_is_unreachable(pool: PgPool) {
    let rig = Rig::new(pool).await;
    let descriptor = provider_descriptor("boamp").unwrap();
    rig.save(
        descriptor,
        json!({"base_url": "http://127.0.0.1:9/records"}),
        &[],
    )
    .await;

    let tested = rig
        .engine
        .test(&rig.uc, &rig.cipher, "boamp")
        .await
        .unwrap();

    assert_eq!(tested.status, ProviderStatus::Failing);
    assert!(tested.last_test_message.unwrap().contains("injoignable"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_api_key_is_injected_as_a_prefixed_header(pool: PgPool) {
    let rig = Rig::with_catalog(pool, Some(&API_KEY_CATALOG)).await;
    rig.save(&API_KEY_CATALOG[0], json!({}), &[("api_key", "abc-123")])
        .await;

    let credential = rig
        .engine
        .credential_for(&rig.uc, &rig.cipher, "keyed")
        .await
        .unwrap();

    match credential {
        Credential::Header { name, value } => {
            assert_eq!(name, "X-Api-Key");
            assert_eq!(value.expose(), "Token abc-123");
        }
        other => panic!("expected a header credential, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_missing_api_key_is_reported(pool: PgPool) {
    let rig = Rig::with_catalog(pool, Some(&API_KEY_CATALOG)).await;
    rig.save(&API_KEY_CATALOG[0], json!({}), &[("api_key", "abc-123")])
        .await;
    let configuration = rig
        .uc
        .get_provider_configuration("keyed")
        .await
        .unwrap()
        .unwrap();
    let stripped = ProviderConfiguration {
        secrets: Default::default(),
        ..configuration
    };

    let error = rig
        .engine
        .credential(&rig.uc, &rig.cipher, &stripped)
        .await
        .unwrap_err();

    assert_eq!(error, AuthFailure::MissingCredential("api_key"));
}
