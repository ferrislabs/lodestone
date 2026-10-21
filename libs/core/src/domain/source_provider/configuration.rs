use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::descriptor::SourceProviderDescriptor;
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

pub fn attestation_stamp_key(field: &str) -> String {
    format!("{field}_at")
}

pub fn stamp_attestations(
    descriptor: &SourceProviderDescriptor,
    previous: Option<&Map<String, Value>>,
    settings: &mut Map<String, Value>,
    now: DateTime<Utc>,
) {
    for field in descriptor
        .fields
        .iter()
        .filter(|field| field.is_attestation())
    {
        let stamp = attestation_stamp_key(field.key);
        if settings.get(field.key) != Some(&Value::Bool(true)) {
            settings.remove(&stamp);
            continue;
        }
        let kept = previous
            .filter(|previous| previous.get(field.key) == Some(&Value::Bool(true)))
            .and_then(|previous| previous.get(&stamp))
            .filter(|value| value.is_string())
            .cloned();
        settings.insert(
            stamp,
            kept.unwrap_or_else(|| Value::String(now.to_rfc3339())),
        );
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
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;
    use crate::domain::source_provider::catalog::descriptor;

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

    fn settings(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    }

    fn jungle() -> &'static SourceProviderDescriptor {
        descriptor("welcome_to_the_jungle").unwrap()
    }

    #[test]
    fn a_checked_attestation_is_stamped_once() {
        let first = Utc.with_ymd_and_hms(2026, 10, 9, 8, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 10, 12, 8, 0, 0).unwrap();
        let mut saved = settings(json!({"authorised": true}));
        stamp_attestations(jungle(), None, &mut saved, first);
        assert_eq!(saved["authorised_at"], first.to_rfc3339());
        let mut again = settings(json!({"authorised": true}));
        stamp_attestations(jungle(), Some(&saved), &mut again, later);
        assert_eq!(again["authorised_at"], first.to_rfc3339());
        let mut unchecked = settings(json!({"authorised": false}));
        stamp_attestations(jungle(), Some(&saved), &mut unchecked, later);
        assert!(!unchecked.contains_key("authorised_at"));
        let mut rechecked = settings(json!({"authorised": true}));
        stamp_attestations(jungle(), Some(&unchecked), &mut rechecked, later);
        assert_eq!(rechecked["authorised_at"], later.to_rfc3339());
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
