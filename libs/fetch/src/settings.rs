use std::{num::NonZeroU32, time::Duration};

use common::OutboundConfig;
use lodestone_core::SourceName;
use rate_limit::Quota;

const DEFAULT_MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_MAX_REDIRECTS: u8 = 5;
const DEFAULT_ROBOTS_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
pub struct FetchSettings {
    pub contact: String,
    pub requests_per_host_per_minute: NonZeroU32,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    pub max_redirects: u8,
    pub robots_ttl: Duration,
    pub source_quotas: Vec<(SourceName, Quota)>,
    pub allow_private_targets: bool,
}

impl FetchSettings {
    pub fn from_outbound(config: &OutboundConfig) -> Self {
        Self {
            contact: config.contact.as_str().to_owned(),
            requests_per_host_per_minute: config.requests_per_host_per_minute,
            request_timeout: config.request_timeout,
            max_body_bytes: DEFAULT_MAX_BODY_BYTES,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            robots_ttl: DEFAULT_ROBOTS_TTL,
            source_quotas: Vec::new(),
            allow_private_targets: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use common::OutboundContact;

    use super::*;

    #[test]
    fn from_outbound_fills_defaults() {
        let config = OutboundConfig {
            contact: OutboundContact::parse("bot@example.com").unwrap(),
            requests_per_host_per_minute: NonZeroU32::new(30).unwrap(),
            revisit_interval: Duration::from_secs(1),
            request_timeout: Duration::from_secs(7),
        };
        let settings = FetchSettings::from_outbound(&config);
        assert_eq!(settings.contact, "bot@example.com");
        assert_eq!(settings.requests_per_host_per_minute.get(), 30);
        assert_eq!(settings.request_timeout, Duration::from_secs(7));
        assert_eq!(settings.max_body_bytes, 2 * 1024 * 1024);
        assert_eq!(settings.max_redirects, 5);
        assert_eq!(settings.robots_ttl, Duration::from_secs(86_400));
        assert!(settings.source_quotas.is_empty());
        assert!(!settings.allow_private_targets);
    }
}
