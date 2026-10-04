use std::{
    fmt::{Debug, Formatter, Result as FmtResult},
    num::{NonZeroU16, NonZeroU32},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use thiserror::Error;
use zeroize::Zeroize;

use crate::Secret;

pub const ENCRYPTION_KEY_LENGTH: usize = 32;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("LLM_SECRETS_ENCRYPTION_KEY must be base64 for exactly {ENCRYPTION_KEY_LENGTH} bytes")]
    InvalidEncryptionKey,
    #[error("JOBS_LEASE_SECONDS must be longer than JOBS_POLL_INTERVAL_MS")]
    LeaseNotLongerThanPoll,
    #[error("OUTBOUND_CONTACT must be an email address or an https URL")]
    InvalidOutboundContact,
}

#[derive(Clone)]
pub struct EncryptionKey([u8; ENCRYPTION_KEY_LENGTH]);

impl EncryptionKey {
    pub fn from_base64(encoded: &Secret) -> Result<Self, SettingsError> {
        let mut decoded = STANDARD
            .decode(encoded.expose().trim())
            .map_err(|_| SettingsError::InvalidEncryptionKey)?;
        let key = <[u8; ENCRYPTION_KEY_LENGTH]>::try_from(decoded.as_slice())
            .map_err(|_| SettingsError::InvalidEncryptionKey);
        decoded.zeroize();
        key.map(Self)
    }

    pub fn expose(&self) -> &[u8; ENCRYPTION_KEY_LENGTH] {
        &self.0
    }
}

impl Drop for EncryptionKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Debug for EncryptionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("EncryptionKey([redacted])")
    }
}

#[derive(Clone, Debug)]
pub struct JobsConfig {
    pub workers: NonZeroU16,
    pub lease: Duration,
    pub poll_interval: Duration,
    pub max_attempts: NonZeroU16,
    pub retry_base_delay: Duration,
    pub shutdown_grace: Duration,
    pub source_interval: Duration,
}

impl JobsConfig {
    pub fn validated(self) -> Result<Self, SettingsError> {
        if self.lease <= self.poll_interval {
            return Err(SettingsError::LeaseNotLongerThanPoll);
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutboundContact(Box<str>);

impl OutboundContact {
    pub fn parse(raw: &str) -> Result<Self, SettingsError> {
        let raw = raw.trim();
        if is_email(raw) || is_https_url(raw) {
            Ok(Self(raw.into()))
        } else {
            Err(SettingsError::InvalidOutboundContact)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_email(raw: &str) -> bool {
    match raw.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !raw.contains(char::is_whitespace)
        }
        None => false,
    }
}

fn is_https_url(raw: &str) -> bool {
    raw.strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

#[derive(Clone, Debug)]
pub struct OutboundConfig {
    pub contact: OutboundContact,
    pub requests_per_host_per_minute: NonZeroU32,
    pub revisit_interval: Duration,
    pub request_timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct LlmConfig {
    pub encryption_key: EncryptionKey,
    pub run_budget_cents: NonZeroU32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz16(value: u16) -> NonZeroU16 {
        NonZeroU16::new(value).unwrap()
    }

    #[test]
    fn encryption_key_accepts_32_base64_bytes() {
        let encoded = Secret::new(STANDARD.encode([7u8; ENCRYPTION_KEY_LENGTH]));
        let key = EncryptionKey::from_base64(&encoded).unwrap();
        assert_eq!(key.expose(), &[7u8; ENCRYPTION_KEY_LENGTH]);
    }

    #[test]
    fn encryption_key_rejects_wrong_length_and_bad_encoding() {
        let short = Secret::new(STANDARD.encode([7u8; 16]));
        assert_eq!(
            EncryptionKey::from_base64(&short).unwrap_err(),
            SettingsError::InvalidEncryptionKey
        );
        assert_eq!(
            EncryptionKey::from_base64(&Secret::new("not base64!")).unwrap_err(),
            SettingsError::InvalidEncryptionKey
        );
    }

    #[test]
    fn encryption_key_never_appears_in_debug_output() {
        let encoded = STANDARD.encode([0xABu8; ENCRYPTION_KEY_LENGTH]);
        let llm = LlmConfig {
            encryption_key: EncryptionKey::from_base64(&Secret::new(encoded.clone())).unwrap(),
            run_budget_cents: NonZeroU32::new(500).unwrap(),
        };
        let printed = format!("{llm:?}");
        assert!(!printed.contains(&encoded));
        assert!(!printed.contains("171"));
        assert!(printed.contains("redacted"));
    }

    #[test]
    fn jobs_lease_must_exceed_poll_interval() {
        let result = JobsConfig {
            workers: nz16(4),
            lease: Duration::from_secs(1),
            poll_interval: Duration::from_secs(1),
            max_attempts: nz16(5),
            retry_base_delay: Duration::from_secs(10),
            shutdown_grace: Duration::from_secs(20),
            source_interval: Duration::from_secs(3600),
        }
        .validated();
        assert_eq!(result.unwrap_err(), SettingsError::LeaseNotLongerThanPoll);
    }

    #[test]
    fn outbound_contact_accepts_email_or_https_url() {
        assert!(OutboundContact::parse("bot@ferrislabs.dev").is_ok());
        assert!(OutboundContact::parse("https://ferrislabs.dev/lodestone").is_ok());
    }

    #[test]
    fn outbound_contact_rejects_anything_else() {
        for raw in [
            "",
            "ferrislabs",
            "a@b",
            "http://x.dev",
            "a b@x.dev",
            "@x.dev",
        ] {
            assert_eq!(
                OutboundContact::parse(raw).unwrap_err(),
                SettingsError::InvalidOutboundContact,
                "{raw}"
            );
        }
    }
}
