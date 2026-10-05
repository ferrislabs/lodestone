use std::time::Duration;

use common::Secret;
use thiserror::Error;
use url::Url;
use uuid::Uuid;

pub const MIN_SECRET_BYTES: usize = 32;
pub const MAX_TIMEOUT_SECONDS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WebhookConfigError {
    #[error("WEBHOOK_URL must be an absolute http or https URL")]
    InvalidUrl,
    #[error("WEBHOOK_SECRET is required when WEBHOOK_URL is set")]
    MissingSecret,
    #[error("WEBHOOK_SECRET must be at least {MIN_SECRET_BYTES} bytes")]
    ShortSecret,
    #[error("WEBHOOK_PROFILE_IDS must be a comma separated list of profile UUIDs")]
    InvalidProfileId,
    #[error("WEBHOOK_TIMEOUT_SECONDS must be between 1 and {MAX_TIMEOUT_SECONDS}")]
    InvalidTimeout,
}

#[derive(Clone)]
pub struct WebhookSettings {
    pub url: Url,
    pub secret: Secret,
    pub profile_ids: Vec<Uuid>,
    pub timeout: Duration,
}

impl std::fmt::Debug for WebhookSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebhookSettings")
            .field("host", &self.url.host_str())
            .field("secret", &self.secret)
            .field("profile_ids", &self.profile_ids)
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(clap::Args, Debug, Clone)]
pub struct WebhookArgs {
    #[arg(
        long = "webhook-url",
        env = "WEBHOOK_URL",
        name = "WEBHOOK_URL",
        hide_env_values = true,
        long_help = "Endpoint receiving a signed POST for each validated lead. Webhooks are disabled when unset. Operator-configured, internal hosts are allowed"
    )]
    pub url: Option<Secret>,

    #[arg(
        long = "webhook-secret",
        env = "WEBHOOK_SECRET",
        name = "WEBHOOK_SECRET",
        hide_env_values = true,
        long_help = "HMAC-SHA256 key signing each delivery, at least 32 bytes, mounted from a Kubernetes secret. Required when WEBHOOK_URL is set"
    )]
    pub secret: Option<Secret>,

    #[arg(
        long = "webhook-profile-ids",
        env = "WEBHOOK_PROFILE_IDS",
        name = "WEBHOOK_PROFILE_IDS",
        value_delimiter = ',',
        long_help = "Comma separated profile UUIDs to deliver, every profile when empty"
    )]
    pub profile_ids: Vec<String>,

    #[arg(
        long = "webhook-timeout-seconds",
        env = "WEBHOOK_TIMEOUT_SECONDS",
        name = "WEBHOOK_TIMEOUT_SECONDS",
        default_value_t = 10,
        long_help = "Seconds a delivery may take before it is retried"
    )]
    pub timeout_seconds: u64,
}

impl WebhookArgs {
    pub fn settings(&self) -> Result<Option<WebhookSettings>, WebhookConfigError> {
        let Some(raw_url) = self
            .url
            .as_ref()
            .map(|url| url.expose().trim())
            .filter(|url| !url.is_empty())
        else {
            return Ok(None);
        };
        let url = Url::parse(raw_url).map_err(|_| WebhookConfigError::InvalidUrl)?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(WebhookConfigError::InvalidUrl);
        }
        let secret = self
            .secret
            .as_ref()
            .filter(|secret| !secret.expose().is_empty())
            .ok_or(WebhookConfigError::MissingSecret)?;
        if secret.expose().len() < MIN_SECRET_BYTES {
            return Err(WebhookConfigError::ShortSecret);
        }
        let profile_ids = self
            .profile_ids
            .iter()
            .map(|raw| raw.trim())
            .filter(|raw| !raw.is_empty())
            .map(|raw| Uuid::parse_str(raw).map_err(|_| WebhookConfigError::InvalidProfileId))
            .collect::<Result<Vec<_>, _>>()?;
        if self.timeout_seconds == 0 || self.timeout_seconds > MAX_TIMEOUT_SECONDS {
            return Err(WebhookConfigError::InvalidTimeout);
        }
        Ok(Some(WebhookSettings {
            url,
            secret: secret.clone(),
            profile_ids,
            timeout: Duration::from_secs(self.timeout_seconds),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        webhook: WebhookArgs,
    }

    const SECRET: &str = "0123456789abcdef0123456789abcdef";
    const PROFILE: &str = "0197c3a0-0000-7000-8000-000000000001";

    fn parse(extra: &[&str]) -> Result<Option<WebhookSettings>, WebhookConfigError> {
        let mut argv = vec!["cmd"];
        argv.extend_from_slice(extra);
        Cmd::try_parse_from(argv).unwrap().webhook.settings()
    }

    #[test]
    fn webhooks_are_disabled_without_a_url() {
        assert!(parse(&[]).unwrap().is_none());
        assert!(parse(&["--webhook-url", ""]).unwrap().is_none());
        assert!(parse(&["--webhook-secret", SECRET]).unwrap().is_none());
    }

    #[test]
    fn a_url_without_a_secret_is_rejected() {
        assert_eq!(
            parse(&["--webhook-url", "https://example.com/hook"]).unwrap_err(),
            WebhookConfigError::MissingSecret
        );
    }

    #[test]
    fn a_short_secret_is_rejected() {
        assert_eq!(
            parse(&[
                "--webhook-url",
                "https://example.com/hook",
                "--webhook-secret",
                "too-short"
            ])
            .unwrap_err(),
            WebhookConfigError::ShortSecret
        );
    }

    #[test]
    fn a_valid_configuration_uses_the_documented_defaults() {
        let settings = parse(&[
            "--webhook-url",
            "http://crm.internal:8080/hook",
            "--webhook-secret",
            SECRET,
        ])
        .unwrap()
        .unwrap();
        assert_eq!(settings.url.host_str(), Some("crm.internal"));
        assert!(settings.profile_ids.is_empty());
        assert_eq!(settings.timeout, Duration::from_secs(10));
    }

    #[test]
    fn the_profile_filter_is_parsed_and_validated() {
        let base = [
            "--webhook-url",
            "https://example.com/hook",
            "--webhook-secret",
            SECRET,
        ];
        let mut ok = base.to_vec();
        ok.extend(["--webhook-profile-ids", PROFILE]);
        assert_eq!(
            parse(&ok).unwrap().unwrap().profile_ids,
            vec![Uuid::parse_str(PROFILE).unwrap()]
        );
        let mut bad = base.to_vec();
        bad.extend(["--webhook-profile-ids", "nope"]);
        assert_eq!(
            parse(&bad).unwrap_err(),
            WebhookConfigError::InvalidProfileId
        );
    }

    #[test]
    fn non_http_urls_and_bad_timeouts_are_rejected() {
        assert_eq!(
            parse(&[
                "--webhook-url",
                "ftp://example.com/hook",
                "--webhook-secret",
                SECRET
            ])
            .unwrap_err(),
            WebhookConfigError::InvalidUrl
        );
        assert_eq!(
            parse(&[
                "--webhook-url",
                "https://example.com/hook",
                "--webhook-secret",
                SECRET,
                "--webhook-timeout-seconds",
                "0"
            ])
            .unwrap_err(),
            WebhookConfigError::InvalidTimeout
        );
    }

    #[test]
    fn debug_output_never_prints_the_secret_or_the_url() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--webhook-url",
            "https://example.com/hook?token=abc",
            "--webhook-secret",
            SECRET,
        ])
        .unwrap();
        let settings = cmd.webhook.settings().unwrap().unwrap();
        assert!(!format!("{settings:?}").contains(SECRET));
        assert!(!format!("{settings:?}").contains("token=abc"));
        assert!(!format!("{:?}", cmd.webhook).contains(SECRET));
        assert!(!format!("{:?}", cmd.webhook).contains("token=abc"));
    }
}
