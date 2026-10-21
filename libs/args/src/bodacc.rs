use clap::Args;
use url::Url;

#[derive(Args, Debug, Clone)]
pub struct BodaccArgs {
    #[arg(
        long = "bodacc-url",
        env = "BODACC_URL",
        name = "BODACC_URL",
        default_value = "https://bodacc-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/annonces-commerciales",
        long_help = "OpenDataSoft dataset of the BODACC commercial announcements, read by SIREN under /records; the provider catalog overrides it"
    )]
    pub url: Url,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        bodacc: BodaccArgs,
    }

    #[test]
    fn defaults_to_the_official_dataset() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(
            cmd.bodacc.url.host_str(),
            Some("bodacc-datadila.opendatasoft.com")
        );
        assert!(cmd.bodacc.url.path().ends_with("/annonces-commerciales"));
    }
}
