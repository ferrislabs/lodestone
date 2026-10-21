use std::time::Duration;

#[derive(clap::Args, Debug, Clone)]
pub struct EnrichmentArgs {
    #[arg(
        long = "enrichment-wait-minutes",
        env = "ENRICHMENT_WAIT_MINUTES",
        name = "ENRICHMENT_WAIT_MINUTES",
        default_value_t = 30,
        long_help = "Minutes after its creation during which a lead waits for its enabled enrichment steps (HTTP probe, company job offers) before qualification; 0 disables the wait"
    )]
    pub wait_minutes: u64,
}

impl EnrichmentArgs {
    pub fn wait(&self) -> Duration {
        Duration::from_secs(self.wait_minutes.saturating_mul(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        enrichment: EnrichmentArgs,
    }

    #[test]
    fn leads_wait_thirty_minutes_for_their_enrichment_by_default() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.enrichment.wait_minutes, 30);
        assert_eq!(cmd.enrichment.wait(), Duration::from_secs(1800));
        assert_eq!(
            Cmd::parse_from(["cmd", "--enrichment-wait-minutes", "0"])
                .enrichment
                .wait(),
            Duration::ZERO
        );
    }
}
