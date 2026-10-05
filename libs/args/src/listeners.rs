use std::{num::NonZeroUsize, time::Duration};

#[derive(clap::Args, Debug, Clone)]
pub struct ListenerArgs {
    #[arg(
        long = "listener-idle-seconds",
        env = "LISTENER_IDLE_SECONDS",
        name = "LISTENER_IDLE_SECONDS",
        default_value_t = 15,
        value_parser = clap::value_parser!(u64).range(1..),
        long_help = "Seconds a continuous listener sleeps when it found nothing to process"
    )]
    pub idle_seconds: u64,

    #[arg(
        long = "listener-lock-retry-seconds",
        env = "LISTENER_LOCK_RETRY_SECONDS",
        name = "LISTENER_LOCK_RETRY_SECONDS",
        default_value_t = 15,
        value_parser = clap::value_parser!(u64).range(1..),
        long_help = "Seconds a standby replica waits before trying again to take over a listener"
    )]
    pub lock_retry_seconds: u64,

    #[arg(
        long = "probe-concurrency",
        env = "PROBE_CONCURRENCY",
        name = "PROBE_CONCURRENCY",
        default_value = "8",
        long_help = "Hosts the HTTP probe listener probes at the same time; per-host rate limits still apply"
    )]
    pub probe_concurrency: NonZeroUsize,

    #[arg(
        long = "dns-concurrency",
        env = "DNS_CONCURRENCY",
        name = "DNS_CONCURRENCY",
        default_value = "16",
        long_help = "Hosts the DNS CNAME listener resolves at the same time"
    )]
    pub dns_concurrency: NonZeroUsize,
}

impl ListenerArgs {
    pub fn idle(&self) -> Duration {
        Duration::from_secs(self.idle_seconds)
    }

    pub fn lock_retry(&self) -> Duration {
        Duration::from_secs(self.lock_retry_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        listeners: ListenerArgs,
    }

    #[test]
    fn defaults_are_fifteen_seconds() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.listeners.idle(), Duration::from_secs(15));
        assert_eq!(cmd.listeners.lock_retry(), Duration::from_secs(15));
    }

    #[test]
    fn concurrency_defaults_and_zero_rejection() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.listeners.probe_concurrency.get(), 8);
        assert_eq!(cmd.listeners.dns_concurrency.get(), 16);
        assert!(Cmd::try_parse_from(["cmd", "--probe-concurrency", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--dns-concurrency", "0"]).is_err());
    }

    #[test]
    fn zero_durations_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--listener-idle-seconds", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--listener-lock-retry-seconds", "0"]).is_err());
    }
}
