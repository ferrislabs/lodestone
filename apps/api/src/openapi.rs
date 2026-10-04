use utoipa::{
    Modify, OpenApi,
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use handlers_organization as organization;

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
    info(title = "Oxid API", description = "API for Oxid", version = "0.1.0"),
    paths(
        organization::bootstrap::handler,
        organization::create::handler,
        organization::list_mine::handler,
        organization::update::handler,
    ),
    components(schemas(
        organization::create::CreateOrganizationRequest,
        organization::update::UpdateOrganizationRequest,
        organization::response::OrganizationResponse,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "organizations", description = "Organizations management"),
    )
)]
pub struct ApiDoc;
