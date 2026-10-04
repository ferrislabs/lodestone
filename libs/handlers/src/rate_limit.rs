use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::{
    HeaderName, HeaderValue, StatusCode,
    header::{FORWARDED, RETRY_AFTER},
};
use rate_limit::{RateLimitDecision, RateLimitKey};
use tracing::{Span, field, warn};

use crate::{errors::ApiError, state::AppState};

const X_RATELIMIT_LIMIT: HeaderName = HeaderName::from_static("x-ratelimit-limit");
const X_RATELIMIT_REMAINING: HeaderName = HeaderName::from_static("x-ratelimit-remaining");
const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const X_REAL_IP: HeaderName = HeaderName::from_static("x-real-ip");

#[tracing::instrument(
    name = "rate_limit.check",
    skip_all,
    fields(
        otel.kind = "internal",
        client.ip = field::Empty,
        rate_limit.scope = "ip",
        rate_limit.limit = field::Empty,
        rate_limit.remaining = field::Empty,
        rate_limit.allowed = field::Empty,
        rate_limit.outcome = field::Empty,
    )
)]
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    ConnectInfo(socket): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let span = Span::current();
    let ip = client_ip(&req, socket, state.args.server.trust_forwarded_headers);
    span.record("client.ip", ip.as_str());

    let key = RateLimitKey::ip(&ip);
    let quota = state.rate_limit_quota;
    span.record("rate_limit.limit", quota.limit);

    let decision = match state.rate_limit.check(&key, quota).await {
        Ok(d) => d,
        Err(e) => {
            // Fail-open: a hiccupping Redis must not take the whole API down.
            span.record("rate_limit.outcome", "backend_error");
            warn!(error = %e, "rate limiter unavailable, allowing request");
            return next.run(req).await;
        }
    };

    span.record("rate_limit.allowed", decision.allowed);
    span.record("rate_limit.remaining", decision.remaining);

    if !decision.allowed {
        span.record("rate_limit.outcome", "blocked");
        return rate_limited_response(&decision);
    }

    span.record("rate_limit.outcome", "allowed");
    let mut response = next.run(req).await;
    inject_headers(response.headers_mut(), &decision);
    response
}

/// Identifies the client a rate-limit bucket belongs to.
///
/// Forwarding headers are attacker-controlled unless a reverse proxy overwrites
/// them, so they are consulted only when the deployment says so. Trusting them
/// by default let any client pick a fresh bucket per request - and pick someone
/// else's to exhaust it.
///
/// Whatever is chosen is parsed as an address: an unparseable value would
/// otherwise become a Redis key of its own, letting a caller inflate the key
/// space with arbitrary strings. IPv6 is aggregated to its /64 prefix, since a
/// single client is routinely handed far more than one address in that range.
#[doc(hidden)]
pub fn client_ip(req: &Request, fallback: SocketAddr, trust_forwarded: bool) -> String {
    let candidate = if trust_forwarded {
        forwarded_candidate(req)
    } else {
        None
    };

    candidate
        .and_then(|raw| IpAddr::from_str(&raw).ok())
        .map_or_else(|| normalize(fallback.ip()), normalize)
}

fn forwarded_candidate(req: &Request) -> Option<String> {
    if let Some(value) = req.headers().get(&X_FORWARDED_FOR)
        && let Ok(s) = value.to_str()
        && let Some(first) = s.split(',').next()
    {
        let trimmed = first.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }

    if let Some(value) = req.headers().get(&X_REAL_IP)
        && let Ok(s) = value.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_owned());
        }
    }

    if let Some(value) = req.headers().get(FORWARDED)
        && let Ok(s) = value.to_str()
    {
        for part in s.split(';') {
            if let Some(rest) = part.trim().strip_prefix("for=") {
                let cleaned = rest
                    .trim_matches('"')
                    .trim_matches(|c| c == '[' || c == ']');
                if !cleaned.is_empty() {
                    return Some(cleaned.to_owned());
                }
            }
        }
    }

    None
}

/// Collapses an IPv6 address onto its /64, leaves IPv4 untouched.
fn normalize(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
    }
}

fn rate_limited_response(decision: &RateLimitDecision) -> Response {
    let mut response = ApiError::TooManyRequests.into_response();
    let headers = response.headers_mut();
    inject_headers(headers, decision);
    if let Some(retry_after) = decision.retry_after {
        let secs = retry_after.as_secs().max(1);
        let mut buf = itoa::Buffer::new();
        if let Ok(value) = HeaderValue::from_str(buf.format(secs)) {
            headers.insert(RETRY_AFTER, value);
        }
    }
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    response
}

#[doc(hidden)]
pub fn inject_headers(headers: &mut http::HeaderMap, decision: &RateLimitDecision) {
    let mut buf = itoa::Buffer::new();
    if let Ok(v) = HeaderValue::from_str(buf.format(decision.limit)) {
        headers.insert(X_RATELIMIT_LIMIT, v);
    }
    if let Ok(v) = HeaderValue::from_str(buf.format(decision.remaining)) {
        headers.insert(X_RATELIMIT_REMAINING, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    fn request_with(header: &str, value: &str) -> Request {
        Request::builder()
            .header(header, value)
            .body(Body::empty())
            .expect("build request")
    }

    fn socket() -> SocketAddr {
        SocketAddr::from(([203, 0, 113, 7], 1234))
    }

    #[test]
    fn forwarding_headers_are_ignored_unless_the_deployment_trusts_them() {
        let req = request_with("x-forwarded-for", "198.51.100.9");
        assert_eq!(client_ip(&req, socket(), false), "203.0.113.7");
    }

    #[test]
    fn a_trusted_forwarding_header_identifies_the_client() {
        let req = request_with("x-forwarded-for", "198.51.100.9, 10.0.0.1");
        assert_eq!(client_ip(&req, socket(), true), "198.51.100.9");
    }

    #[test]
    fn an_unparseable_forwarded_value_falls_back_to_the_socket() {
        // Otherwise an arbitrary string becomes a Redis key of its own.
        let req = request_with("x-forwarded-for", "not-an-address");
        assert_eq!(client_ip(&req, socket(), true), "203.0.113.7");
    }

    #[test]
    fn addresses_in_one_ipv6_prefix_share_a_bucket() {
        let first = request_with("x-forwarded-for", "2001:db8:1:2:3:4:5:6");
        let second = request_with("x-forwarded-for", "2001:db8:1:2:ffff:ffff:ffff:ffff");
        assert_eq!(
            client_ip(&first, socket(), true),
            client_ip(&second, socket(), true)
        );
    }

    #[test]
    fn the_real_ip_header_is_honoured_when_trusted() {
        let req = request_with("x-real-ip", "198.51.100.9");
        assert_eq!(client_ip(&req, socket(), true), "198.51.100.9");
    }
}
