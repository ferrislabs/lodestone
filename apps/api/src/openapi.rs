use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Lodestone API", description = "Lead detection API", version = "0.1.0"),
    paths(
        handlers::api::me,
        handlers::profiles::list_profiles,
        handlers::profiles::get_profile,
        handlers::profiles::list_revisions,
        handlers::profiles::get_revision,
        handlers::profiles::create_profile,
        handlers::profiles::import_profile,
        handlers::profiles::update_profile,
        handlers::profiles::activate_profile,
        handlers::profiles::deactivate_profile,
        handlers::profiles::export_profile,
        handlers::leads::list_leads,
        handlers::exports::export_leads,
        handlers::leads::lead_detail,
        handlers::leads::change_lead_status,
        handlers::drafts::list_drafts,
        handlers::drafts::get_draft,
        handlers::drafts::edit_draft,
        handlers::drafts::approve_draft,
        handlers::drafts::reject_draft,
        handlers::llm::list_providers,
        handlers::llm::get_provider,
        handlers::llm::create_provider,
        handlers::llm::update_provider,
        handlers::llm::enable_provider,
        handlers::llm::disable_provider,
        handlers::llm::delete_provider,
        handlers::llm::test_provider,
        handlers::llm::list_tiers,
        handlers::llm::assign_tier,
        handlers::llm::unassign_tier,
    ),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

#[cfg(test)]
mod tests {
    use utoipa::OpenApi;

    use super::ApiDoc;

    const METHODS: [&str; 8] = [
        "get", "put", "post", "delete", "options", "head", "patch", "trace",
    ];

    #[test]
    fn every_business_operation_requires_bearer_auth() {
        let spec = serde_json::to_value(ApiDoc::openapi()).expect("serialize spec");
        let paths = spec["paths"].as_object().expect("paths object");

        let business: Vec<_> = paths
            .iter()
            .filter(|(path, _)| path.starts_with("/api/v1/"))
            .collect();
        assert!(!business.is_empty());

        for (path, item) in business {
            let operations: Vec<_> = METHODS
                .iter()
                .filter_map(|method| item.get(*method).map(|op| (*method, op)))
                .collect();
            assert!(!operations.is_empty(), "{path} declares no operation");

            for (method, operation) in operations {
                let secured = operation["security"]
                    .as_array()
                    .is_some_and(|reqs| reqs.iter().any(|req| req.get("bearer_auth").is_some()));
                assert!(secured, "{method} {path} lacks the bearer_auth requirement");
            }
        }
    }

    #[test]
    fn me_is_documented() {
        let spec = ApiDoc::openapi();
        assert!(spec.paths.paths.contains_key("/api/v1/me"));
    }
}
