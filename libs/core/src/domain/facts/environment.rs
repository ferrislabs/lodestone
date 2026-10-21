use crate::domain::host::Hostname;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Environment {
    Production,
    Development,
    Staging,
    Test,
    Preproduction,
    Demo,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Test => "test",
            Self::Preproduction => "preproduction",
            Self::Demo => "demo",
        }
    }

    pub fn is_production(self) -> bool {
        self == Self::Production
    }
}

fn classify(token: &str) -> Option<Environment> {
    let base = token.trim_end_matches(|c: char| c.is_ascii_digit());
    match base {
        "dev" | "develop" | "development" => Some(Environment::Development),
        "stg" | "stage" | "staging" => Some(Environment::Staging),
        "test" | "testing" | "qa" | "uat" | "int" | "recette" | "rec" => Some(Environment::Test),
        "preprod" | "preproduction" => Some(Environment::Preproduction),
        "sandbox" | "demo" => Some(Environment::Demo),
        _ => None,
    }
}

fn classify_label(label: &str) -> Option<Environment> {
    let tokens: Vec<&str> = label.split('-').collect();
    if tokens.windows(2).any(|pair| {
        pair[0] == "pre" && pair[1].trim_end_matches(|c: char| c.is_ascii_digit()) == "prod"
    }) {
        return Some(Environment::Preproduction);
    }
    tokens.into_iter().find_map(classify)
}

fn subdomain(host: &Hostname) -> &str {
    let name = host.as_str();
    match host.registrable_domain() {
        Some(domain) => name
            .strip_suffix(domain.as_str())
            .unwrap_or_default()
            .trim_end_matches('.'),
        None => name.rsplit_once('.').map_or("", |(rest, _)| rest),
    }
}

pub fn environment_of(host: &Hostname) -> Environment {
    subdomain(host)
        .split('.')
        .filter(|label| !label.is_empty())
        .find_map(classify_label)
        .unwrap_or(Environment::Production)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(host: &str) -> Environment {
        environment_of(&Hostname::parse(host).unwrap())
    }

    #[test]
    fn a_whole_label_names_the_environment() {
        assert_eq!(env("api.dev.example.com"), Environment::Development);
        assert_eq!(env("login.dev.luzernrisk.com"), Environment::Development);
        assert_eq!(env("develop.example.com"), Environment::Development);
        assert_eq!(env("stg.example.com"), Environment::Staging);
        assert_eq!(env("stage.example.com"), Environment::Staging);
        assert_eq!(env("staging.example.com"), Environment::Staging);
        assert_eq!(env("test.example.com"), Environment::Test);
        assert_eq!(env("testing.example.com"), Environment::Test);
        assert_eq!(env("qa.example.com"), Environment::Test);
        assert_eq!(env("uat.example.com"), Environment::Test);
        assert_eq!(env("int.example.com"), Environment::Test);
        assert_eq!(env("recette.example.com"), Environment::Test);
        assert_eq!(env("rec.example.com"), Environment::Test);
        assert_eq!(env("preprod.example.com"), Environment::Preproduction);
        assert_eq!(env("pre-prod.example.com"), Environment::Preproduction);
        assert_eq!(env("sandbox.example.com"), Environment::Demo);
        assert_eq!(env("demo.example.com"), Environment::Demo);
    }

    #[test]
    fn a_dash_separated_token_names_the_environment() {
        assert_eq!(env("dev-api.example.com"), Environment::Development);
        assert_eq!(env("api-dev.example.com"), Environment::Development);
        assert_eq!(env("api-staging-eu.example.com"), Environment::Staging);
        assert_eq!(env("api-pre-prod.example.com"), Environment::Preproduction);
        assert_eq!(env("app-preprod.example.com"), Environment::Preproduction);
    }

    #[test]
    fn a_numeric_suffix_keeps_the_environment() {
        assert_eq!(env("dev2.example.com"), Environment::Development);
        assert_eq!(env("test01.example.com"), Environment::Test);
        assert_eq!(env("api-stg3.example.com"), Environment::Staging);
        assert_eq!(env("pre-prod2.example.com"), Environment::Preproduction);
    }

    #[test]
    fn a_word_that_merely_contains_a_marker_is_production() {
        assert_eq!(env("devops.example.com"), Environment::Production);
        assert_eq!(env("latest.example.com"), Environment::Production);
        assert_eq!(env("contest.example.com"), Environment::Production);
        assert_eq!(env("integration-hub.example.com"), Environment::Production);
        assert_eq!(env("demonstration.example.com"), Environment::Production);
        assert_eq!(env("prod.example.com"), Environment::Production);
        assert_eq!(env("pre.example.com"), Environment::Production);
        assert_eq!(env("sso.example.com"), Environment::Production);
    }

    #[test]
    fn the_registrable_domain_is_never_inspected() {
        assert_eq!(env("dev.com"), Environment::Production);
        assert_eq!(env("www.test.com"), Environment::Production);
        assert_eq!(env("staging.co.uk"), Environment::Production);
        assert_eq!(env("api.staging.co.uk"), Environment::Production);
        assert_eq!(env("dev.api.staging.co.uk"), Environment::Development);
    }

    #[test]
    fn the_leftmost_marker_wins() {
        assert_eq!(env("dev.staging.example.com"), Environment::Development);
        assert_eq!(env("staging.dev.example.com"), Environment::Staging);
    }

    #[test]
    fn the_apex_is_production() {
        assert_eq!(env("example.com"), Environment::Production);
        assert!(env("www.example.com").is_production());
    }

    #[test]
    fn a_host_without_registrable_domain_ignores_its_last_label() {
        assert_eq!(env("corp.internal"), Environment::Production);
        assert_eq!(env("dev.corp.internal"), Environment::Development);
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(Environment::Preproduction.as_str(), "preproduction");
        assert_eq!(Environment::Production.as_str(), "production");
    }
}
