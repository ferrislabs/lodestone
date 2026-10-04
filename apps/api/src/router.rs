use axum::{Router, extract::Request};
use http::{
    HeaderValue, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION},
};
use tower_http::{catch_panic::CatchPanicLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::info_span;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};
use utoipa_swagger_ui::SwaggerUi;

use axum::middleware::from_fn_with_state;
use handlers::{ApiError, AppState, rate_limit::rate_limit_middleware};
use handlers_organization as organization;

use crate::openapi::ApiDoc;

/// Parameter names present in a query string, comma separated. Names describe
/// the shape of a request; values are the payload.
fn query_keys(query: Option<&str>) -> String {
    query
        .map(|q| {
            q.split('&')
                .filter_map(|pair| pair.split('=').next())
                .filter(|key| !key.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

pub fn router(state: AppState) -> Result<Router, ApiError> {
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request| {
            let method = request.method();
            let path = request.uri().path();
            let span = info_span!(
                "http_request",
                otel.name = %format!("{method} {path}"),
                otel.kind = "server",
                http.request.method = %method,
                url.path = %path,
                // The raw query string is deliberately absent. It reached the
                // trace backend verbatim, where retention and access control
                // are looser than the database's, and an ERP puts customer
                // names and identifiers in query parameters. Record the keys,
                // never the values.
                url.query_keys = %query_keys(request.uri().query()),
                http.response.status_code = tracing::field::Empty,
            );
            span
        })
        .on_response(
            |response: &http::Response<_>, _latency: std::time::Duration, span: &tracing::Span| {
                span.record("http.response.status_code", response.status().as_u16());
            },
        );

    let openapi = ApiDoc::openapi();

    // Read from configuration: the origin used to be compiled in, pointing at a
    // development port this project does not even use, so no deployment could
    // ever be reached by its own frontend.
    let allowed_origins = state
        .args
        .server
        .allowed_origins
        .iter()
        .map(|origin| HeaderValue::from_str(origin))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ApiError::BadRequest("invalid allowed origin".to_owned()))?;

    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::PUT,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_origin(allowed_origins)
        .allow_headers([
            AUTHORIZATION,
            CONTENT_TYPE,
            CONTENT_LENGTH,
            ACCEPT,
            LOCATION,
        ])
        .allow_credentials(true);

    let mut router = Router::new().merge(organization::router(&state));

    if state.args.server.enable_api_docs {
        router = router
            .merge(Scalar::with_url("/scalar", openapi.clone()))
            .merge(SwaggerUi::new("/swagger").url("/api-docs/openapi.json", openapi.clone()));
    }

    let router = router
        // Outside authentication, so a flood of unauthenticated requests is
        // throttled before any token is validated - which costs a round trip
        // to the identity provider on a cache miss.
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(trace_layer)
        .layer(cors)
        // Last resort: a panicking handler must return 500, not drop the
        // connection and leave the caller guessing.
        .layer(CatchPanicLayer::new())
        .with_state(state);

    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::query_keys;

    #[test]
    fn only_parameter_names_are_kept() {
        // The values are the payload: an ERP puts customer names and
        // identifiers in query parameters, and traces outlive requests.
        assert_eq!(query_keys(Some("search=Dupont&page=2")), "search,page");
    }

    #[test]
    fn an_absent_or_empty_query_yields_nothing() {
        assert_eq!(query_keys(None), "");
        assert_eq!(query_keys(Some("")), "");
        assert_eq!(query_keys(Some("&&")), "");
    }

    #[test]
    fn a_bare_flag_still_reports_its_name() {
        assert_eq!(query_keys(Some("verbose")), "verbose");
    }
}
