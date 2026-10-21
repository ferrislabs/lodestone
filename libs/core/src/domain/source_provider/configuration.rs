use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::domain::{lead::str_enum, llm::provider::SealedSecret};

pub const TOKEN_ACCESS: &str = "oauth_access_token";
pub const TOKEN_REFRESH: &str = "oauth_refresh_token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderStatus {
    NotConfigured,
    Untested,
    Ready,
    Failing,
}

str_enum!(ProviderStatus {
    NotConfigured => "not_configured",
    Untested => "untested",
    Ready => "ready",
    Failing => "failing",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub display_name: String,
    pub account_id: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub connected_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderConfiguration {
    pub key: String,
    pub settings: Map<String, Value>,
    pub secrets: BTreeMap<String, SealedSecret>,
    pub status: ProviderStatus,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_message: Option<String>,
    pub connected_account: Option<ConnectedAccount>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProviderConfiguration {
    pub fn is_usable(&self) -> bool {
        self.status == ProviderStatus::Ready
    }

    pub fn has_secret(&self, name: &str) -> bool {
        self.secrets.contains_key(name)
    }
}

pub fn secret_context(provider_key: &str, name: &str) -> Vec<u8> {
    format!("source-provider:{provider_key}:{name}").into_bytes()
}

const MASK: &str = "••••";
const REVEALED_CHARS: usize = 4;
const MIN_CHARS_TO_REVEAL: usize = 12;

pub fn mask_secret(value: &str) -> String {
    let count = value.chars().count();
    if count < MIN_CHARS_TO_REVEAL {
        return MASK.to_owned();
    }
    let tail: String = value.chars().skip(count - REVEALED_CHARS).collect();
    format!("{MASK}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_secrets_reveal_their_last_four_characters() {
        assert_eq!(mask_secret("sk-live-0123456789-abcd"), "••••abcd");
    }

    #[test]
    fn short_secrets_reveal_nothing() {
        assert_eq!(mask_secret("abcdefgh"), "••••");
        assert_eq!(mask_secret(""), "••••");
    }

    #[test]
    fn multibyte_secrets_are_cut_on_characters() {
        assert_eq!(mask_secret("ééééééééééééé"), "••••éééé");
    }

    #[test]
    fn status_round_trips() {
        for status in [
            ProviderStatus::NotConfigured,
            ProviderStatus::Untested,
            ProviderStatus::Ready,
            ProviderStatus::Failing,
        ] {
            assert_eq!(ProviderStatus::parse(status.as_str()), Some(status));
        }
    }

    #[test]
    fn contexts_differ_per_provider_and_name() {
        assert_ne!(secret_context("a", "x"), secret_context("b", "x"));
        assert_ne!(secret_context("a", "x"), secret_context("a", "y"));
    }
}
