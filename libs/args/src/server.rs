use std::vec;

#[derive(clap::Args, Debug, Clone)]
pub struct ServerArgs {
    #[arg(
        short,
        long,
        env,
        num_args = 0..,
        value_delimiter = ',',
        long_help = "Comma-separated origins allowed to call the API from a browser, for example http://localhost:3000. Empty allows none, so the web UI cannot reach the API.",
    )]
    pub allowed_origins: Vec<String>,
    #[arg(
        long = "server-enable-api-docs",
        env = "SERVER_ENABLE_API_DOCS",
        name = "SERVER_ENABLE_API_DOCS",
        default_value_t = false,
        long_help = "Serve the Scalar and Swagger interfaces and the OpenAPI document. \
They sit outside authentication, so enabling them publishes the full API surface \
to anonymous callers."
    )]
    pub enable_api_docs: bool,

    #[arg(
        long = "server-trust-forwarded-headers",
        env = "SERVER_TRUST_FORWARDED_HEADERS",
        name = "SERVER_TRUST_FORWARDED_HEADERS",
        default_value_t = false,
        long_help = "Trust X-Forwarded-For / X-Real-IP / Forwarded to identify the client. \
Only safe when every request reaches this process through a reverse proxy that \
overwrites those headers; otherwise any client can choose its own rate-limit \
bucket, or exhaust someone else's."
    )]
    pub trust_forwarded_headers: bool,

    #[arg(
        short = 'H',
        long = "server-host",
        env = "SERVER_HOST",
        name = "SERVER_HOST",
        default_value = "0.0.0.0",
        long_help = "The host to run the application on"
    )]
    pub host: String,
    #[arg(
        short = 'P',
        long = "server-port",
        env = "SERVER_PORT",
        name = "SERVER_PORT",
        default_value_t = 3456,
        long_help = "The port to run the application on"
    )]
    pub port: u16,
    #[arg(
        long = "server-internal-host",
        env = "SERVER_INTERNAL_HOST",
        name = "SERVER_INTERNAL_HOST",
        default_value = "127.0.0.1",
        long_help = "Interface the internal router binds to. It carries health and \
metrics and has no authentication, so it defaults to the loopback address."
    )]
    pub internal_host: String,
    #[arg(
        long = "server-internal-port",
        env = "SERVER_INTERNAL_PORT",
        name = "SERVER_INTERNAL_PORT",
        default_value_t = 3457,
        long_help = "The port to run the internal application on (health, metrics, ...)"
    )]
    pub internal_port: u16,
    #[arg(
        long = "server-public-url",
        env = "SERVER_PUBLIC_URL",
        name = "SERVER_PUBLIC_URL",
        default_value = "http://localhost:3456",
        long_help = "Base URL of this API as a browser reaches it. The OAuth redirect URI \
registered at a source provider is this URL followed by /api/v1/source-providers/oauth/callback."
    )]
    pub public_url: String,
    #[arg(
        long = "server-webapp-url",
        env = "SERVER_WEBAPP_URL",
        name = "SERVER_WEBAPP_URL",
        default_value = "http://localhost:3000",
        long_help = "Base URL of the web UI. The OAuth callback redirects the browser to the provider page under this URL."
    )]
    pub webapp_url: String,
}

impl Default for ServerArgs {
    fn default() -> Self {
        Self {
            allowed_origins: vec![],
            enable_api_docs: false,
            trust_forwarded_headers: false,
            host: "0.0.0.0".to_string(),
            port: 3333,
            internal_host: "127.0.0.1".to_string(),
            internal_port: 3334,
            public_url: "http://localhost:3456".to_string(),
            webapp_url: "http://localhost:3000".to_string(),
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
        server: ServerArgs,
    }

    #[test]
    fn default_values() {
        let args = ServerArgs::default();
        assert_eq!(args.host, "0.0.0.0");
        assert_eq!(args.port, 3333);
        assert_eq!(args.internal_port, 3334);
        assert!(args.allowed_origins.is_empty());
    }

    #[test]
    fn public_and_webapp_urls_default_to_local_development() {
        let cmd = Cmd::try_parse_from(["cmd"]).unwrap();
        assert_eq!(cmd.server.public_url, "http://localhost:3456");
        assert_eq!(cmd.server.webapp_url, "http://localhost:3000");
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--server-public-url",
            "https://api.example.com",
            "--server-webapp-url",
            "https://app.example.com",
        ])
        .unwrap();
        assert_eq!(cmd.server.public_url, "https://api.example.com");
        assert_eq!(cmd.server.webapp_url, "https://app.example.com");
    }

    #[test]
    fn parse_host_and_port() {
        let cmd =
            Cmd::try_parse_from(["cmd", "--server-host", "127.0.0.1", "--server-port", "8080"])
                .unwrap();
        assert_eq!(cmd.server.host, "127.0.0.1");
        assert_eq!(cmd.server.port, 8080);
    }

    #[test]
    fn parse_allowed_origins_comma_separated() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--allowed-origins",
            "https://app.example.com,https://admin.example.com",
        ])
        .unwrap();
        assert_eq!(
            cmd.server.allowed_origins,
            vec!["https://app.example.com", "https://admin.example.com"]
        );
    }

    #[test]
    fn parse_allowed_origins_repeated_flag() {
        let cmd = Cmd::try_parse_from([
            "cmd",
            "--allowed-origins",
            "https://a.example.com",
            "--allowed-origins",
            "https://b.example.com",
        ])
        .unwrap();
        assert_eq!(cmd.server.allowed_origins.len(), 2);
    }
}
