use common::{DatabaseConfig, Secret};
use url::Url;

#[derive(clap::Args, Debug, Clone)]
pub struct DatabaseArgs {
    #[arg(
        long = "database-host",
        env = "DATABASE_HOST",
        default_value = "localhost",
        name = "DATABASE_HOST",
        long_help = "The database host to use"
    )]
    pub host: String,
    #[arg(
        long = "database-name",
        env = "DATABASE_NAME",
        name = "DATABASE_NAME",
        long_help = "The database name to use"
    )]
    pub name: String,
    #[arg(
        long = "database-password",
        env = "DATABASE_PASSWORD",
        default_value = "oxid",
        name = "DATABASE_PASSWORD",
        long_help = "The database password. Required: a default would let a \
deployment that forgets it start silently with a value anyone can read in this \
repository."
    )]
    pub password: Secret,
    #[arg(
        long = "database-port",
        env = "DATABASE_PORT",
        default_value_t = 5432,
        name = "DATABASE_PORT",
        long_help = "The database port to use"
    )]
    pub port: u16,
    #[arg(
        long = "database-user",
        env = "DATABASE_USER",
        default_value = "oxid",
        name = "DATABASE_USER",
        long_help = "The database user to use"
    )]
    pub user: String,
}

impl Default for DatabaseArgs {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            name: "oxid".to_string(),
            password: Secret::new("oxid"),
            port: 5432,
            user: "oxid".to_string(),
        }
    }
}

impl From<Url> for DatabaseArgs {
    fn from(value: Url) -> Self {
        Self {
            host: value
                .host()
                .unwrap_or(url::Host::Domain("localhost"))
                .to_string(),
            name: value.path().to_string(),
            password: Secret::new(value.password().unwrap_or("")),
            port: value.port().unwrap_or(5432),
            user: value.username().to_string(),
        }
    }
}

impl From<DatabaseArgs> for DatabaseConfig {
    fn from(value: DatabaseArgs) -> Self {
        Self {
            host: value.host,
            name: value.name,
            password: value.password,
            port: value.port,
            username: value.user,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let args = DatabaseArgs::default();
        assert_eq!(args.host, "localhost");
        assert_eq!(args.name, "oxid");
        assert_eq!(args.password.expose(), "oxid");
        assert_eq!(args.port, 5432);
        assert_eq!(args.user, "oxid");
    }

    #[test]
    fn from_full_url() {
        let url = Url::parse("postgres://alice:secret@db.example.com:5433/mydb").unwrap();
        let args = DatabaseArgs::from(url);
        assert_eq!(args.host, "db.example.com");
        assert_eq!(args.name, "/mydb");
        assert_eq!(args.password.expose(), "secret");
        assert_eq!(args.port, 5433);
        assert_eq!(args.user, "alice");
    }

    #[test]
    fn from_url_missing_password_uses_empty_string() {
        let url = Url::parse("postgres://alice@db.example.com/mydb").unwrap();
        let args = DatabaseArgs::from(url);
        assert_eq!(args.password.expose(), "");
    }

    #[test]
    fn from_url_missing_port_uses_5432() {
        let url = Url::parse("postgres://alice:secret@db.example.com/mydb").unwrap();
        let args = DatabaseArgs::from(url);
        assert_eq!(args.port, 5432);
    }

    #[test]
    fn from_url_localhost() {
        let url = Url::parse("postgres://user:pass@localhost:5432/testdb").unwrap();
        let args = DatabaseArgs::from(url);
        assert_eq!(args.host, "localhost");
        assert_eq!(args.port, 5432);
    }
}
