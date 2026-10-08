use std::time::Duration;

#[derive(clap::Args, Debug, Clone)]
pub struct EnrichmentArgs {
    #[arg(
        long = "requalify-debounce-seconds",
        env = "REQUALIFY_DEBOUNCE_SECONDS",
        name = "REQUALIFY_DEBOUNCE_SECONDS",
        default_value_t = 120,
        long_help = "Seconds a lead waits after the last new finding of its ecosystem inspection or the last change of its company dossier before it is evaluated again, so several modules finishing together give one requalification"
    )]
    pub requalify_debounce_seconds: u64,

    #[arg(
        long = "requalify-max-per-day",
        env = "REQUALIFY_MAX_PER_DAY",
        name = "REQUALIFY_MAX_PER_DAY",
        default_value_t = 3,
        long_help = "Automatic requalifications of one lead per day (UTC); beyond, the lead waits for the next day or a manual « Réévaluer »"
    )]
    pub requalify_max_per_day: u32,

    #[arg(
        long = "inspection-priority-batch",
        env = "INSPECTION_PRIORITY_BATCH",
        name = "INSPECTION_PRIORITY_BATCH",
        default_value_t = 30,
        long_help = "Domains inspected first by each module per pass: those never inspected or with a lead created less than an hour ago"
    )]
    pub inspection_priority_batch: u32,
}

impl EnrichmentArgs {
    pub fn requalify_debounce(&self) -> Duration {
        Duration::from_secs(self.requalify_debounce_seconds)
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
    fn requalification_waits_two_minutes_three_times_a_day_by_default() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(
            cmd.enrichment.requalify_debounce(),
            Duration::from_secs(120)
        );
        assert_eq!(cmd.enrichment.requalify_max_per_day, 3);
        assert_eq!(cmd.enrichment.inspection_priority_batch, 30);
        let tuned = Cmd::parse_from([
            "cmd",
            "--requalify-debounce-seconds",
            "0",
            "--requalify-max-per-day",
            "5",
            "--inspection-priority-batch",
            "50",
        ]);
        assert_eq!(tuned.enrichment.requalify_debounce(), Duration::ZERO);
        assert_eq!(tuned.enrichment.requalify_max_per_day, 5);
        assert_eq!(tuned.enrichment.inspection_priority_batch, 50);
    }
}
