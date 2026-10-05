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
    paths(handlers::api::me),
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
