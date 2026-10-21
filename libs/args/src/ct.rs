use std::num::{NonZeroU32, NonZeroU64};

use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct CtArgs {
    #[arg(
        long = "ct-log-list-url",
        env = "CT_LOG_LIST_URL",
        name = "CT_LOG_LIST_URL",
        default_value = "https://www.gstatic.com/ct/log_list/v3/log_list.json",
        long_help = "Chrome log list giving the tiled Certificate Transparency logs to read"
    )]
    pub log_list_url: Url,

    #[arg(
        long = "ct-max-logs",
        env = "CT_MAX_LOGS",
        name = "CT_MAX_LOGS",
        default_value_t = 8,
        long_help = "Maximum number of tiled logs read, in log list order by description"
    )]
    pub max_logs: usize,

    #[arg(
        long = "ct-tiles-per-run",
        env = "CT_TILES_PER_RUN",
        name = "CT_TILES_PER_RUN",
        default_value = "64",
        long_help = "Maximum data tiles (256 entries each) read per log in one run; keep it below CT_REQUESTS_PER_HOST_PER_MINUTE so a pass fits in about a minute"
    )]
    pub tiles_per_run: NonZeroU64,

    #[arg(
        long = "ct-max-lag-tiles",
        env = "CT_MAX_LAG_TILES",
        name = "CT_MAX_LAG_TILES",
        default_value = "256",
        long_help = "A log further behind than this many tiles skips ahead to stay current"
    )]
    pub max_lag_tiles: NonZeroU64,

    #[arg(
        long = "ct-requests-per-host-per-minute",
        env = "CT_REQUESTS_PER_HOST_PER_MINUTE",
        name = "CT_REQUESTS_PER_HOST_PER_MINUTE",
        default_value = "120",
        long_help = "Maximum requests per minute to one Certificate Transparency log host, independent of OUTBOUND_REQUESTS_PER_HOST_PER_MINUTE"
    )]
    pub requests_per_host_per_minute: NonZeroU32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        ct: CtArgs,
    }

    #[test]
    fn defaults() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.ct.max_logs, 8);
        assert_eq!(cmd.ct.tiles_per_run.get(), 64);
        assert_eq!(cmd.ct.max_lag_tiles.get(), 256);
        assert_eq!(cmd.ct.requests_per_host_per_minute.get(), 120);
        assert_eq!(cmd.ct.log_list_url.host_str(), Some("www.gstatic.com"));
    }

    #[test]
    fn zero_tiles_per_run_is_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--ct-tiles-per-run", "0"]).is_err());
    }
}
