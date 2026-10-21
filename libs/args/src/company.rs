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
}

impl CompanyArgs {
    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.refresh_days.get()) * 86_400)
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
    fn defaults_read_five_pages_and_refresh_every_ninety_days() {
        let cmd = Cmd::parse_from(["cmd"]);
        assert_eq!(cmd.company.max_pages.get(), 5);
        assert_eq!(cmd.company.refresh_days.get(), 90);
        assert_eq!(
            cmd.company.refresh_interval(),
            Duration::from_secs(90 * 86_400)
        );
    }

    #[test]
    fn pages_and_days_cannot_be_zero() {
        assert!(Cmd::try_parse_from(["cmd", "--company-research-max-pages", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--company-research-refresh-days", "0"]).is_err());
    }
}
