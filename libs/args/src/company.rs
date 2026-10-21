use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

#[derive(clap::Args, Debug, Clone)]
pub struct CompanyArgs {
    #[arg(
        long = "company-research-max-pages",
        env = "COMPANY_RESEARCH_MAX_PAGES",
        name = "COMPANY_RESEARCH_MAX_PAGES",
        default_value = "5",
        long_help = "Pages of the company site read for one dossier, home page included"
    )]
    pub max_pages: NonZeroUsize,

    #[arg(
        long = "company-research-refresh-days",
        env = "COMPANY_RESEARCH_REFRESH_DAYS",
        name = "COMPANY_RESEARCH_REFRESH_DAYS",
        default_value = "90",
        long_help = "Days before the dossier of a domain is built again"
    )]
    pub refresh_days: NonZeroU32,

    #[arg(
        long = "company-research-wait-minutes",
        env = "COMPANY_RESEARCH_WAIT_MINUTES",
        name = "COMPANY_RESEARCH_WAIT_MINUTES",
        default_value_t = 10,
        long_help = "Minutes after its creation during which the qualification agents wait for the company dossier of a lead; 0 disables the wait"
    )]
    pub wait_minutes: u64,
}

impl CompanyArgs {
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.refresh_days.get()) * 86_400)
    }

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
        company: CompanyArgs,
    }

    #[test]
    fn defaults_read_five_pages_refresh_every_ninety_days_and_wait_ten_minutes() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.company.max_pages.get(), 5);
        assert_eq!(cmd.company.refresh_days.get(), 90);
        assert_eq!(cmd.company.wait_minutes, 10);
        assert_eq!(
            cmd.company.refresh_interval(),
            Duration::from_secs(90 * 86_400)
        );
        assert_eq!(cmd.company.wait(), Duration::from_secs(600));
    }

    #[test]
    fn the_wait_can_be_disabled_but_pages_and_days_cannot_be_zero() {
        assert_eq!(
            Cmd::parse_from(["cmd", "--company-research-wait-minutes", "0"])
                .company
                .wait(),
            Duration::ZERO
        );
        assert!(Cmd::try_parse_from(["cmd", "--company-research-max-pages", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--company-research-refresh-days", "0"]).is_err());
    }
}
