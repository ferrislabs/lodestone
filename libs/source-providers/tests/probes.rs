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

async fn tested(rig: &Rig, key: &str) -> ProviderConfiguration {
    rig.engine.test(&rig.uc, &rig.cipher, key).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn decp_is_ready_when_the_dataset_api_answers(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("decp").unwrap(),
        json!({"dataset_api_url": rig.server.url("/dataset")}),
        &[],
    )
    .await;
    let dataset = rig
        .server
        .mock_async(|when, then| {
            when.method(GET).path("/dataset");
            then.status(200).json_body(json!({"id": "decp"}));
        })
        .await;

    let configuration = tested(&rig, "decp").await;

    assert_eq!(configuration.status, ProviderStatus::Ready);
    assert!(rig.uc.is_provider_usable("decp").await.unwrap());
    dataset.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn decp_fails_when_the_dataset_api_answers_an_error(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("decp").unwrap(),
        json!({"dataset_api_url": rig.server.url("/dataset")}),
        &[],
    )
    .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/dataset");
            then.status(404);
        })
        .await;

    let configuration = tested(&rig, "decp").await;

    assert_eq!(configuration.status, ProviderStatus::Failing);
    assert!(configuration.last_test_message.unwrap().contains("404"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn ted_accepts_any_answer_below_a_server_error(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("ted").unwrap(),
        json!({"base_url": rig.server.url("/search")}),
        &[],
    )
    .await;
    let search = rig
        .server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(405);
        })
        .await;

    let configuration = tested(&rig, "ted").await;

    assert_eq!(configuration.status, ProviderStatus::Ready);
    search.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn ted_fails_on_a_server_error(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("ted").unwrap(),
        json!({"base_url": rig.server.url("/search")}),
        &[],
    )
    .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(502);
        })
        .await;

    assert_eq!(tested(&rig, "ted").await.status, ProviderStatus::Failing);
}

#[sqlx::test(migrations = "../../migrations")]
async fn certificate_transparency_is_ready_when_the_log_list_answers(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("certificate_transparency").unwrap(),
        json!({"log_list_url": rig.server.url("/log_list.json")}),
        &[],
    )
    .await;
    let list = rig
        .server
        .mock_async(|when, then| {
            when.method(GET).path("/log_list.json");
            then.status(200).json_body(json!({"operators": []}));
        })
        .await;

    let configuration = tested(&rig, "certificate_transparency").await;

    assert_eq!(configuration.status, ProviderStatus::Ready);
    list.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn certificate_transparency_fails_when_the_log_list_is_unreachable(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("certificate_transparency").unwrap(),
        json!({"log_list_url": "http://127.0.0.1:9/log_list.json"}),
        &[],
    )
    .await;

    let configuration = tested(&rig, "certificate_transparency").await;

    assert_eq!(configuration.status, ProviderStatus::Failing);
    assert!(
        configuration
            .last_test_message
            .unwrap()
            .contains("injoignable")
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_research_queries_the_search_endpoint(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("company_research").unwrap(),
        json!({"entreprises_url": rig.server.url("/search")}),
        &[],
    )
    .await;
    let search = rig
        .server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/search")
                .query_param("per_page", "1");
            then.status(200).json_body(json!({"results": []}));
        })
        .await;

    let configuration = tested(&rig, "company_research").await;

    assert_eq!(configuration.status, ProviderStatus::Ready);
    search.assert_async().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_research_fails_on_a_rate_limit_answer(pool: PgPool) {
    let rig = Rig::new(pool).await;
    rig.save(
        provider_descriptor("company_research").unwrap(),
        json!({"entreprises_url": rig.server.url("/search")}),
        &[],
    )
    .await;
    rig.server
        .mock_async(|when, then| {
            when.method(GET).path("/search");
            then.status(429);
        })
        .await;

    let configuration = tested(&rig, "company_research").await;

    assert_eq!(configuration.status, ProviderStatus::Failing);
    assert!(configuration.last_test_message.unwrap().contains("429"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn crawler_style_providers_have_no_external_endpoint_and_always_pass(pool: PgPool) {
    let rig = Rig::new(pool).await;
    for key in ["ats", "dns_cname", "http_probe", "contacts"] {
        rig.save(provider_descriptor(key).unwrap(), json!({}), &[])
            .await;

        let configuration = tested(&rig, key).await;

        assert_eq!(configuration.status, ProviderStatus::Ready, "{key}");
        assert!(
            configuration
                .last_test_message
                .unwrap()
                .contains("aucun point d'accès externe"),
            "{key}"
        );
    }
}
