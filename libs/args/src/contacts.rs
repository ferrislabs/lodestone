#[derive(clap::Args, Debug, Clone)]
pub struct ContactArgs {
    #[arg(
        long = "contact-page-paths",
        env = "CONTACT_PAGE_PATHS",
        name = "CONTACT_PAGE_PATHS",
        value_delimiter = ',',
        default_value = "/,/contact,/mentions-legales,/legal,/about",
        long_help = "Comma-separated paths crawled on each lead domain to find contact addresses"
    )]
    pub page_paths: Vec<String>,

    #[arg(
        long = "contact-batch-size",
        env = "CONTACT_BATCH_SIZE",
        name = "CONTACT_BATCH_SIZE",
        default_value_t = 10,
        long_help = "Lead domains crawled per contact run"
    )]
    pub batch_size: u32,

    #[arg(
        long = "contact-max-rate-limit-wait-seconds",
        env = "CONTACT_MAX_RATE_LIMIT_WAIT_SECONDS",
        name = "CONTACT_MAX_RATE_LIMIT_WAIT_SECONDS",
        default_value_t = 75,
        long_help = "Longest per-host rate-limit wait accepted while crawling one domain; longer waits end the run"
    )]
    pub max_rate_limit_wait_seconds: u64,
}

impl ContactArgs {
    pub fn max_rate_limit_wait(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.max_rate_limit_wait_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        contacts: ContactArgs,
    }

    #[test]
    fn default_paths_cover_contact_and_legal_pages() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert!(
            cmd.contacts
                .page_paths
                .contains(&"/mentions-legales".to_owned())
        );
        assert_eq!(cmd.contacts.page_paths[0], "/");
        assert_eq!(cmd.contacts.batch_size, 10);
    }

    #[test]
    fn paths_are_configurable() {
        let cmd =
            Cmd::try_parse_from(["cmd", "--contact-page-paths", "/kontakt,/impressum"]).unwrap();
        assert_eq!(cmd.contacts.page_paths, ["/kontakt", "/impressum"]);
    }
}
