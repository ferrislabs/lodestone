use std::{
    num::{NonZeroU32, NonZeroU64},
    time::Duration,
};

use url::Url;

const BYTES_PER_MB: u64 = 1024 * 1024;

#[derive(clap::Args, Debug, Clone)]
pub struct DecpArgs {
    #[arg(
        long = "decp-dataset-api-url",
        env = "DECP_DATASET_API_URL",
        name = "DECP_DATASET_API_URL",
        default_value = "https://www.data.gouv.fr/api/1/datasets/donnees-essentielles-de-la-commande-publique-fichiers-consolides/",
        long_help = "data.gouv.fr API URL of the consolidated DECP dataset, listing one JSON file per month"
    )]
    pub dataset_api_url: Url,

    #[arg(
        long = "decp-annuaire-url",
        env = "DECP_ANNUAIRE_URL",
        name = "DECP_ANNUAIRE_URL",
        default_value = "https://api-lannuaire.service-public.fr/api/explore/v2.1/catalog/datasets/api-lannuaire-administration/records",
        long_help = "Annuaire de l'administration records endpoint, used to resolve a buyer SIRET to its website"
    )]
    pub annuaire_url: Url,

    #[arg(
        long = "decp-entreprises-url",
        env = "DECP_ENTREPRISES_URL",
        name = "DECP_ENTREPRISES_URL",
        default_value = "https://recherche-entreprises.api.gouv.fr/search",
        long_help = "Recherche d'entreprises endpoint, used to resolve a SIRET to a company name"
    )]
    pub entreprises_url: Url,

    #[arg(
        long = "decp-lookback-days",
        env = "DECP_LOOKBACK_DAYS",
        name = "DECP_LOOKBACK_DAYS",
        default_value = "31",
        long_help = "Monthly files covering the last N days are downloaded (a window of 31 days reads the current and the previous month)"
    )]
    pub lookback_days: NonZeroU32,

    #[arg(
        long = "decp-notified-within-days",
        env = "DECP_NOTIFIED_WITHIN_DAYS",
        name = "DECP_NOTIFIED_WITHIN_DAYS",
        default_value = "180",
        long_help = "Only contracts notified in the last N days are kept; buyers publish weeks to months late, so this is wider than the file window"
    )]
    pub notified_within_days: NonZeroU32,

    #[arg(
        long = "decp-interval-hours",
        env = "DECP_INTERVAL_HOURS",
        name = "DECP_INTERVAL_HOURS",
        default_value = "24",
        long_help = "Hours between two DECP runs; each run downloads the monthly file(s) again"
    )]
    pub interval_hours: NonZeroU64,

    #[arg(
        long = "decp-max-body-mb",
        env = "DECP_MAX_BODY_MB",
        name = "DECP_MAX_BODY_MB",
        default_value = "256",
        long_help = "Largest monthly DECP file accepted, in megabytes (a full month is about 140 MB)"
    )]
    pub max_body_mb: NonZeroU64,
}

impl DecpArgs {
    pub fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_hours.get().saturating_mul(3600))
    }

    pub fn max_body_bytes(&self) -> usize {
        usize::try_from(self.max_body_mb.get().saturating_mul(BYTES_PER_MB)).unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        decp: DecpArgs,
    }

    #[test]
    fn defaults() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.decp.lookback_days.get(), 31);
        assert_eq!(cmd.decp.notified_within_days.get(), 180);
        assert_eq!(cmd.decp.interval(), Duration::from_secs(24 * 3600));
        assert_eq!(cmd.decp.max_body_bytes(), 256 * 1024 * 1024);
        assert_eq!(
            cmd.decp.dataset_api_url.host_str(),
            Some("www.data.gouv.fr")
        );
        assert_eq!(
            cmd.decp.annuaire_url.host_str(),
            Some("api-lannuaire.service-public.fr")
        );
        assert_eq!(
            cmd.decp.entreprises_url.host_str(),
            Some("recherche-entreprises.api.gouv.fr")
        );
    }

    #[test]
    fn overrides_are_read() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--decp-interval-hours",
            "6",
            "--decp-max-body-mb",
            "64",
        ])
        .unwrap();
        assert_eq!(cmd.decp.interval(), Duration::from_secs(6 * 3600));
        assert_eq!(cmd.decp.max_body_bytes(), 64 * 1024 * 1024);
    }

    #[test]
    fn zero_values_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--decp-interval-hours", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--decp-max-body-mb", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--decp-lookback-days", "0"]).is_err());
        assert!(Cmd::try_parse_from(["cmd", "--decp-notified-within-days", "0"]).is_err());
    }
}
