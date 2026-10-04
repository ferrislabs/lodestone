use common::{AuthConfig, Secret};

#[derive(clap::Args, Debug, Clone)]
pub struct AuthArgs {
    #[arg(
        long = "auth-issuer",
        env = "AUTH_ISSUER",
        name = "AUTH_ISSUER",
        default_value = "http://localhost:3333/realms/oxid",
        long_help = "The issuer URL to use for authentication"
    )]
    pub issuer: String,

    #[arg(
        long = "auth-client-id",
        env = "AUTH_CLIENT_ID",
        name = "AUTH_CLIENT_ID",
        default_value = "oxid",
        long_help = "The client ID to use for authentication"
    )]
    pub client_id: String,

    #[arg(
        long = "auth-client-secret",
        env = "AUTH_CLIENT_SECRET",
        name = "AUTH_CLIENT_SECRET",
        long_help = "The client secret used to talk to the identity provider. \
Required: a default would let a deployment that forgets it start silently with \
a value anyone can read in this repository."
    )]
    pub client_secret: Secret,
}

impl From<AuthArgs> for AuthConfig {
    fn from(value: AuthArgs) -> Self {
        Self {
            issuer: value.issuer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cmd {
        #[command(flatten)]
        auth: AuthArgs,
    }

    /// The client secret has no default on purpose, so parsing without it must
    /// fail rather than silently pick a value published in this repository.
    #[test]
    fn the_client_secret_has_no_default() {
        assert!(Cmd::try_parse_from(["cmd"]).is_err());
    }

    #[test]
    fn parse_defaults() {
        let cmd = Cmd::try_parse_from(["cmd", "--auth-client-secret", "s"]).unwrap();
        assert_eq!(cmd.auth.issuer, "http://localhost:3333/realms/oxid");
        assert_eq!(cmd.auth.client_id, "oxid");
    }

    #[test]
    fn parse_custom_issuer() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--auth-issuer",
            "https://auth.example.com/realms/prod",
            "--auth-client-secret",
            "s",
        ])
        .unwrap();
        assert_eq!(cmd.auth.issuer, "https://auth.example.com/realms/prod");
    }

    #[test]
    fn parse_all_fields() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--auth-issuer",
            "https://sso.example.com",
            "--auth-client-id",
            "oxid",
            "--auth-client-secret",
            "supersecret",
        ])
        .unwrap();
        assert_eq!(cmd.auth.issuer, "https://sso.example.com");
        assert_eq!(cmd.auth.client_id, "oxid");
        assert_eq!(cmd.auth.client_secret.expose(), "supersecret");
    }
}
