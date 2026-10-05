use std::num::NonZeroU32;

use common::Secret;
use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct FranceTravailArgs {
    #[arg(
        long = "france-travail-client-id",
        env = "FRANCE_TRAVAIL_CLIENT_ID",
        name = "FRANCE_TRAVAIL_CLIENT_ID",
        long_help = "OAuth2 client id of the France Travail partner application. \
The source is disabled unless both the id and the secret are set"
    )]
    pub client_id: Option<Secret>,

    #[arg(
        long = "france-travail-client-secret",
        env = "FRANCE_TRAVAIL_CLIENT_SECRET",
        name = "FRANCE_TRAVAIL_CLIENT_SECRET",
        long_help = "OAuth2 client secret of the France Travail partner application, never logged"
    )]
    pub client_secret: Option<Secret>,

    #[arg(
        long = "france-travail-token-url",
        env = "FRANCE_TRAVAIL_TOKEN_URL",
        name = "FRANCE_TRAVAIL_TOKEN_URL",
        default_value = "https://entreprise.francetravail.fr/connexion/oauth2/access_token?realm=%2Fpartenaire",
        long_help = "OAuth2 token endpoint, queried with the client credentials grant"
    )]
    pub token_url: Url,

    #[arg(
        long = "france-travail-api-base-url",
        env = "FRANCE_TRAVAIL_API_BASE_URL",
        name = "FRANCE_TRAVAIL_API_BASE_URL",
        default_value = "https://api.francetravail.io/partenaire/offresdemploi/v2",
        long_help = "Base URL of the Offres d'emploi v2 API, `/offres/search` is appended"
    )]
    pub api_base_url: Url,

    #[arg(
        long = "france-travail-published-since-days",
        env = "FRANCE_TRAVAIL_PUBLISHED_SINCE_DAYS",
        name = "FRANCE_TRAVAIL_PUBLISHED_SINCE_DAYS",
        default_value = "7",
        long_help = "Only offers published in the last N days are fetched, mapped to the closest value the API accepts (1, 3, 7, 14 or 31)"
    )]
    pub published_since_days: NonZeroU32,

    #[arg(
        long = "france-travail-max-pages",
        env = "FRANCE_TRAVAIL_MAX_PAGES",
        name = "FRANCE_TRAVAIL_MAX_PAGES",
        default_value = "20",
        long_help = "Maximum pages of 150 offers fetched per keyword and run (the API stops at index 3149)"
    )]
    pub max_pages: NonZeroU32,
}

impl FranceTravailArgs {
    pub fn credentials(&self) -> Option<(&Secret, &Secret)> {
        let id = self
            .client_id
            .as_ref()
            .filter(|id| !id.expose().is_empty())?;
        let secret = self
            .client_secret
            .as_ref()
            .filter(|secret| !secret.expose().is_empty())?;
        Some((id, secret))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        france_travail: FranceTravailArgs,
    }

    #[test]
    fn defaults_target_the_public_endpoints_without_credentials() {
        let cmd = Cmd::parse_from(["cmd"]);
        let args = cmd.france_travail;
        assert!(args.credentials().is_none());
        assert_eq!(args.published_since_days.get(), 7);
        assert_eq!(args.max_pages.get(), 20);
        assert_eq!(
            args.token_url.host_str(),
            Some("entreprise.francetravail.fr")
        );
        assert_eq!(args.token_url.query(), Some("realm=%2Fpartenaire"));
        assert_eq!(args.api_base_url.host_str(), Some("api.francetravail.io"));
    }

    #[test]
    fn both_credentials_enable_the_source() {
        let cmd = Cmd::parse_from([
            "cmd",
            "--france-travail-client-id",
            "id",
            "--france-travail-client-secret",
            "secret",
        ]);
        let (id, secret) = cmd.france_travail.credentials().unwrap();
        assert_eq!(id.expose(), "id");
        assert_eq!(secret.expose(), "secret");
    }

    #[test]
    fn a_missing_or_empty_half_keeps_the_source_disabled() {
        let only_id = Cmd::parse_from(["cmd", "--france-travail-client-id", "id"]);
        assert!(only_id.france_travail.credentials().is_none());
        let empty = Cmd::parse_from([
            "cmd",
            "--france-travail-client-id",
            "id",
            "--france-travail-client-secret",
            "",
        ]);
        assert!(empty.france_travail.credentials().is_none());
    }

    #[test]
    fn debug_output_hides_the_credentials() {
        let cmd = Cmd::parse_from([
            "cmd",
            "--france-travail-client-id",
            "visible-id",
            "--france-travail-client-secret",
            "visible-secret",
        ]);
        let printed = format!("{:?}", cmd.france_travail);
        assert!(!printed.contains("visible-id"));
        assert!(!printed.contains("visible-secret"));
    }

    #[test]
    fn zero_limits_are_rejected() {
        assert!(Cmd::try_parse_from(["cmd", "--france-travail-max-pages", "0"]).is_err());
        assert!(
            Cmd::try_parse_from(["cmd", "--france-travail-published-since-days", "0"]).is_err()
        );
    }
}
