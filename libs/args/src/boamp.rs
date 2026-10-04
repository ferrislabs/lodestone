use std::num::NonZeroU32;

use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct BoampArgs {
    #[arg(
        long = "boamp-base-url",
        env = "BOAMP_BASE_URL",
        name = "BOAMP_BASE_URL",
        default_value = "https://boamp-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/boamp/records",
        long_help = "OpenDataSoft records endpoint of the BOAMP dataset"
    )]
    pub base_url: Url,

    #[arg(
        long = "boamp-lookback-days",
        env = "BOAMP_LOOKBACK_DAYS",
        name = "BOAMP_LOOKBACK_DAYS",
        default_value = "7",
        long_help = "Only notices published in the last N days are fetched"
    )]
    pub lookback_days: NonZeroU32,

    #[arg(
        long = "boamp-max-pages",
        env = "BOAMP_MAX_PAGES",
        name = "BOAMP_MAX_PAGES",
        default_value = "20",
        long_help = "Maximum pages of 50 notices fetched per run"
    )]
    pub max_pages: NonZeroU32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        boamp: BoampArgs,
    }

    #[test]
    fn defaults() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.boamp.lookback_days.get(), 7);
        assert_eq!(cmd.boamp.max_pages.get(), 20);
        assert_eq!(
            cmd.boamp.base_url.host_str(),
            Some("boamp-datadila.opendatasoft.com")
        );
    }

    #[test]
    fn zero_values_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--boamp-max-pages", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--boamp-lookback-days", "0"]).is_err());
    }
}
