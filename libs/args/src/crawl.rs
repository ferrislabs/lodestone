use clap::Args;
use common::RobotsPolicy;

#[derive(Args, Debug, Clone)]
pub struct CrawlArgs {
    #[arg(
        long = "crawl-robots-policy",
        env = "CRAWL_ROBOTS_POLICY",
        name = "CRAWL_ROBOTS_POLICY",
        default_value = "respect",
        long_help = "`respect` or `ignore`: whether company site reading follows robots.txt. Seeds the organisation setting until an operator saves it in the web UI"
    )]
    pub robots_policy: RobotsPolicy,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        crawl: CrawlArgs,
    }

    #[test]
    fn robots_are_respected_by_default() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.crawl.robots_policy, RobotsPolicy::Respect);
    }

    #[test]
    fn ignore_is_accepted_and_unknown_values_are_rejected() {
        let cmd = Cmd::parse_from(["cmd", "--crawl-robots-policy", "ignore"]);
        assert_eq!(cmd.crawl.robots_policy, RobotsPolicy::Ignore);
        assert!(Cmd::try_parse_from(["cmd", "--crawl-robots-policy", "sometimes"]).is_err());
    }
}
