use common::Secret;
use lodestone_core::FetchResponse;
use serde_json::Value;
use serde_json::from_slice;

use crate::error::AuthFailure;

const DEFAULT_EXPIRES_IN: u64 = 3600;

#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token: Secret,
    pub refresh_token: Option<Secret>,
    pub expires_in: u64,
    pub scopes: Vec<String>,
}

fn non_empty<'a>(body: &'a Value, key: &str) -> Option<&'a str> {
    body.get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

pub fn parse_token(response: &FetchResponse) -> Result<TokenResponse, AuthFailure> {
    if !(200..300).contains(&response.status) {
        return Err(AuthFailure::Rejected(response.status));
    }
    let body: Value = from_slice(&response.body).map_err(|_| AuthFailure::InvalidResponse)?;
    let access_token = non_empty(&body, "access_token").ok_or(AuthFailure::InvalidResponse)?;
    Ok(TokenResponse {
        access_token: Secret::new(access_token),
        refresh_token: non_empty(&body, "refresh_token").map(Secret::new),
        expires_in: body
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_EXPIRES_IN),
        scopes: non_empty(&body, "scope")
            .map(|scope| scope.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::*;

    fn response(status: u16, body: &str) -> FetchResponse {
        FetchResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
            final_url: Url::parse("https://example.com/token").expect("url"),
            truncated: false,
        }
    }

    #[test]
    fn a_complete_response_is_parsed() {
        let token = parse_token(&response(
            200,
            r#"{"access_token":"a","refresh_token":"r","expires_in":60,"scope":"x y"}"#,
        ))
        .expect("token");
        assert_eq!(token.access_token.expose(), "a");
        assert_eq!(token.refresh_token.expect("refresh").expose(), "r");
        assert_eq!(token.expires_in, 60);
        assert_eq!(token.scopes, vec!["x", "y"]);
    }

    #[test]
    fn expiry_defaults_when_absent() {
        let token = parse_token(&response(200, r#"{"access_token":"a"}"#)).expect("token");
        assert_eq!(token.expires_in, DEFAULT_EXPIRES_IN);
        assert!(token.refresh_token.is_none());
    }

    #[test]
    fn rejections_keep_the_status_only() {
        assert_eq!(
            parse_token(&response(401, r#"{"error":"invalid_client"}"#)).unwrap_err(),
            AuthFailure::Rejected(401)
        );
    }

    #[test]
    fn a_missing_access_token_is_invalid() {
        assert_eq!(
            parse_token(&response(200, "{}")).unwrap_err(),
            AuthFailure::InvalidResponse
        );
        assert_eq!(
            parse_token(&response(200, "not json")).unwrap_err(),
            AuthFailure::InvalidResponse
        );
    }
}
