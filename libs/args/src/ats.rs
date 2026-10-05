use std::num::{NonZeroU32, NonZeroUsize};

#[derive(clap::Args, Debug, Clone)]
pub struct AtsArgs {
    #[arg(
        long = "ats-batch-size",
        env = "ATS_BATCH_SIZE",
        name = "ATS_BATCH_SIZE",
        default_value = "20",
        long_help = "Lead domains whose careers page is scanned for a public ATS board per run"
    )]
    pub batch_size: NonZeroU32,

    #[arg(
        long = "ats-revisit-days",
        env = "ATS_REVISIT_DAYS",
        name = "ATS_REVISIT_DAYS",
        default_value = "7",
        long_help = "Days before a lead domain is scanned again for open jobs"
    )]
    pub revisit_days: NonZeroU32,

    #[arg(
        long = "ats-max-career-paths",
        env = "ATS_MAX_CAREER_PATHS",
        name = "ATS_MAX_CAREER_PATHS",
        default_value = "6",
        long_help = "Careers pages tried on a lead domain, after its homepage, to find an ATS board link"
    )]
    pub max_career_paths: NonZeroUsize,
}

impl AtsArgs {
    pub fn revisit_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(u64::from(self.revisit_days.get()) * 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        ats: AtsArgs,
    }

    #[test]
    fn defaults_scan_twenty_domains_weekly_over_six_pages() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.ats.batch_size.get(), 20);
        assert_eq!(cmd.ats.revisit_days.get(), 7);
        assert_eq!(cmd.ats.max_career_paths.get(), 6);
        assert_eq!(
            cmd.ats.revisit_interval(),
            std::time::Duration::from_secs(7 * 86_400)
        );
    }

    #[test]
    fn zero_values_are_rejected_at_parse_time() {
        assert!(Cmd::try_parse_from(["cmd", "--ats-batch-size", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--ats-revisit-days", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--ats-max-career-paths", "0"]).is_err());
    }
}
