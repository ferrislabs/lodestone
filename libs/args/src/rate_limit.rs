use common::RateLimitConfig;

#[derive(clap::Args, Debug, Clone)]
pub struct RateLimitArgs {
    #[arg(
        long = "rate-limit-per-minute",
        env = "RATE_LIMIT_PER_MINUTE",
        name = "RATE_LIMIT_PER_MINUTE",
        default_value_t = 600,
        long_help = "Allowed requests per minute per client IP. One screen of the web UI issues several requests, so a low value locks the UI."
    )]
    pub per_minute: u32,
}

impl From<RateLimitArgs> for RateLimitConfig {
    fn from(value: RateLimitArgs) -> Self {
        Self {
            per_minute: value.per_minute,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        rate_limit: RateLimitArgs,
    }

    #[test]
    fn parse_defaults() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.rate_limit.per_minute, 600);
    }

    #[test]
    fn parse_custom() {
        let cmd = Cmd::try_parse_from(["cmd", "--rate-limit-per-minute", "1000"]).unwrap();
        assert_eq!(cmd.rate_limit.per_minute, 1000);
    }
}
