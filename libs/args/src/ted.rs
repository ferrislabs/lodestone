use std::num::NonZeroU32;

use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct TedArgs {
    #[arg(
        long = "ted-base-url",
        env = "TED_BASE_URL",
        name = "TED_BASE_URL",
        default_value = "https://api.ted.europa.eu/v3/notices/search",
        long_help = "TED Search API endpoint, queried with POST and no API key"
    )]
    pub base_url: Url,

    #[arg(
        long = "ted-lookback-days",
        env = "TED_LOOKBACK_DAYS",
        name = "TED_LOOKBACK_DAYS",
        default_value = "7",
        long_help = "Only notices published in the last N days are fetched"
    )]
    pub lookback_days: NonZeroU32,

    #[arg(
        long = "ted-max-pages",
        env = "TED_MAX_PAGES",
        name = "TED_MAX_PAGES",
        default_value = "10",
        long_help = "Maximum pages of 100 notices fetched per run (the API window stops at 15000 notices)"
    )]
    pub max_pages: NonZeroU32,

    #[arg(
        long = "ted-buyer-countries",
        env = "TED_BUYER_COUNTRIES",
        name = "TED_BUYER_COUNTRIES",
        default_value = "FRA",
        value_delimiter = ',',
        num_args = 0..,
        long_help = "Comma separated ISO 3166-1 alpha-3 buyer countries, empty for every country"
    )]
    pub buyer_countries: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        ted: TedArgs,
    }

    #[test]
    fn defaults_target_french_buyers_on_the_public_api() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.ted.lookback_days.get(), 7);
        assert_eq!(cmd.ted.max_pages.get(), 10);
        assert_eq!(cmd.ted.buyer_countries, ["FRA"]);
        assert_eq!(cmd.ted.base_url.host_str(), Some("api.ted.europa.eu"));
    }

    #[test]
    fn countries_are_a_comma_list_and_can_be_empty() {
        let cmd = Cmd::parse_from(["cmd", "--ted-buyer-countries", "FRA,BEL"]);
        assert_eq!(cmd.ted.buyer_countries, ["FRA", "BEL"]);
        let cmd = Cmd::parse_from(["cmd", "--ted-buyer-countries="]);
        assert!(cmd.ted.buyer_countries.iter().all(|c| c.is_empty()));
    }

    #[test]
    fn zero_limits_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--ted-max-pages", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--ted-lookback-days", "0"]).is_err());
    }
}
