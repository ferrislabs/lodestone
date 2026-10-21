use std::time::Duration;

use axum::{
    Router,
    extract::{Request, State},
    response::{IntoResponse, Response},
    routing::get,
};
use http::{StatusCode, header::CONTENT_TYPE};
use sqlx::PgPool;
use telemetry::Metrics;
use tower_http::trace::TraceLayer;
use tracing::{info_span, warn};

use handlers::ApiError;

const QUEUE_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const QUEUE_QUERY: &str = "SELECT kind, status, COUNT(*)::BIGINT FROM jobs \
    WHERE status IN ('pending', 'running', 'failed') GROUP BY kind, status";

#[derive(Clone)]
struct InternalState {
    metrics: Metrics,
    pool: PgPool,
}

pub fn internal_router(metrics: Metrics, pool: PgPool) -> Result<Router, ApiError> {
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request| {
            let method = request.method();
            let path = request.uri().path();
            let span = info_span!(
                "internal_http_request",
                otel.name = %format!("{method} {path}"),
                otel.kind = "server",
                http.request.method = %method,
                url.path = %path,
                http.response.status_code = tracing::field::Empty,
            );
            span
        })
        .on_response(
            |response: &http::Response<_>, _latency: std::time::Duration, span: &tracing::Span| {
                span.record("http.response.status_code", response.status().as_u16());
            },
        );

    let router = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(scrape))
        .layer(trace_layer)
        .with_state(InternalState { metrics, pool });

    Ok(router)
}

async fn health() -> &'static str {
    "ok"
}

async fn scrape(State(state): State<InternalState>) -> Response {
    refresh_queue_depth(&state).await;
    match state.metrics.render() {
        Ok(body) => ([(CONTENT_TYPE, telemetry::CONTENT_TYPE)], body).into_response(),
        Err(error) => {
            warn!(%error, "metrics could not be encoded");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn refresh_queue_depth(state: &InternalState) {
    let query = sqlx::query_as::<_, (String, String, i64)>(QUEUE_QUERY).fetch_all(&state.pool);
    match tokio::time::timeout(QUEUE_QUERY_TIMEOUT, query).await {
        Ok(Ok(rows)) => state.metrics.set_queue_depth(&rows),
        Ok(Err(error)) => warn!(%error, "job queue depth could not be read"),
        Err(_) => warn!("job queue depth query timed out"),
    }
}
