use std::{num::NonZeroU32, time::Duration};

use common::{OutboundConfig, OutboundContact, SettingsError};

#[derive(clap::Args, Debug, Clone)]
pub struct OutboundArgs {
    #[arg(
        long = "outbound-contact",
        env = "OUTBOUND_CONTACT",
        name = "OUTBOUND_CONTACT",
        long_help = "Email address or https URL put in the User-Agent of every outbound request"
    )]
    pub contact: String,

    #[arg(
        long = "outbound-requests-per-host-per-minute",
        env = "OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE",
        name = "OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE",
        default_value = "6",
        long_help = "Maximum outbound requests to one host per minute"
    )]
    pub requests_per_host_per_minute: NonZeroU32,

    #[arg(
        long = "outbound-revisit-interval-hours",
        env = "OUTBOUND_REVISIT_INTERVAL_HOURS",
        name = "OUTBOUND_REVISIT_INTERVAL_HOURS",
        default_value_t = 720,
        long_help = "Hours before the same host is probed again"
    )]
    pub revisit_interval_hours: u64,

    #[arg(
        long = "outbound-request-timeout-seconds",
        env = "OUTBOUND_REQUEST_TIMEOUT_SECONDS",
        name = "OUTBOUND_REQUEST_TIMEOUT_SECONDS",
        default_value_t = 15,
        long_help = "Timeout of one outbound request"
    )]
    pub request_timeout_seconds: u64,
}

const SECONDS_PER_HOUR: u64 = 3600;

impl TryFrom<OutboundArgs> for OutboundConfig {
    type Error = SettingsError;

    fn try_from(value: OutboundArgs) -> Result<Self, Self::Error> {
        Ok(Self {
            contact: OutboundContact::parse(&value.contact)?,
            requests_per_host_per_minute: value.requests_per_host_per_minute,
            revisit_interval: Duration::from_secs(
                value
                    .revisit_interval_hours
                    .saturating_mul(SECONDS_PER_HOUR),
            ),
            request_timeout: Duration::from_secs(value.request_timeout_seconds),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        outbound: OutboundArgs,
    }

    #[test]
    fn contact_is_required() {
        assert!(Cmd::try_parse_from(["cmd"]).is_err());
    }

    #[test]
    fn defaults_with_contact_build_a_valid_config() {
        let cmd = Cmd::try_parse_from(["cmd", "--outbound-contact", "bot@ferrislabs.dev"]).unwrap();
        let config = OutboundConfig::try_from(cmd.outbound).unwrap();
        assert_eq!(config.contact.as_str(), "bot@ferrislabs.dev");
        assert_eq!(config.requests_per_host_per_minute.get(), 6);
        assert_eq!(config.revisit_interval, Duration::from_secs(720 * 3600));
    }

    #[test]
    fn invalid_contact_is_rejected() {
        let cmd = Cmd::try_parse_from(["cmd", "--outbound-contact", "nobody"]).unwrap();
        assert_eq!(
            OutboundConfig::try_from(cmd.outbound).unwrap_err(),
            SettingsError::InvalidOutboundContact
        );
    }
}
