use std::{fmt, net::IpAddr, str::FromStr};

use thiserror::Error;
use url::{Host, Url};

const MAX_DOMAIN_LENGTH: usize = 253;
const WILDCARD_PREFIX: &str = "*.";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NormalizedDomainError {
    #[error("input is empty")]
    Empty,
    #[error("input is not a valid URL")]
    InvalidUrl,
    #[error("URL has no host")]
    MissingHost,
    #[error("an IP address has no registrable domain")]
    IpAddress,
    #[error("host is not a valid domain name")]
    InvalidHost,
    #[error("host has no registrable domain under a known public suffix")]
    NoRegistrableDomain,
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalizedDomain(Box<str>);

impl NormalizedDomain {
    pub fn from_host(host: &str) -> Result<Self, NormalizedDomainError> {
        let host = host.trim();
        let host = host.strip_prefix(WILDCARD_PREFIX).unwrap_or(host);
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty() {
            return Err(NormalizedDomainError::Empty);
        }
        if host.parse::<IpAddr>().is_ok() {
            return Err(NormalizedDomainError::IpAddress);
        }
        let ascii =
            idna::domain_to_ascii_strict(host).map_err(|_| NormalizedDomainError::InvalidHost)?;
        if ascii.is_empty() || ascii.len() > MAX_DOMAIN_LENGTH {
            return Err(NormalizedDomainError::InvalidHost);
        }
        registrable(&ascii)
    }

    pub fn from_url(input: &str) -> Result<Self, NormalizedDomainError> {
        let url = Url::parse(input.trim()).map_err(|_| NormalizedDomainError::InvalidUrl)?;
        match url.host() {
            Some(Host::Domain(host)) => Self::from_host(host),
            Some(Host::Ipv4(_) | Host::Ipv6(_)) => Err(NormalizedDomainError::IpAddress),
            None => Err(NormalizedDomainError::MissingHost),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn registrable(ascii: &str) -> Result<NormalizedDomain, NormalizedDomainError> {
    let domain = psl::domain(ascii.as_bytes()).ok_or(NormalizedDomainError::NoRegistrableDomain)?;
    if !domain.suffix().is_known() {
        return Err(NormalizedDomainError::NoRegistrableDomain);
    }
    let bytes = domain.trim().as_bytes();
    let text = std::str::from_utf8(bytes).map_err(|_| NormalizedDomainError::InvalidHost)?;
    Ok(NormalizedDomain(text.into()))
}

impl FromStr for NormalizedDomain {
    type Err = NormalizedDomainError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.contains("://") {
            Self::from_url(input)
        } else {
            Self::from_host(input)
        }
    }
}

impl TryFrom<&str> for NormalizedDomain {
    type Error = NormalizedDomainError;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        input.parse()
    }
}

impl AsRef<str> for NormalizedDomain {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NormalizedDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn parse(input: &str) -> Result<String, NormalizedDomainError> {
        input.parse::<NormalizedDomain>().map(|d| d.to_string())
    }

    #[test]
    fn subdomain_of_multi_label_suffix_reduces_to_registrable_domain() {
        assert_eq!(parse("auth.example.co.uk").as_deref(), Ok("example.co.uk"));
    }

    #[test]
    fn url_with_mixed_case_and_path_reduces_to_registrable_domain() {
        assert_eq!(
            parse("https://www.EXAMPLE.co.uk/x").as_deref(),
            Ok("example.co.uk")
        );
    }

    #[test]
    fn trailing_dot_and_wildcard_are_ignored() {
        assert_eq!(parse("*.sso.example.fr.").as_deref(), Ok("example.fr"));
    }

    #[test]
    fn url_with_port_and_credentials_keeps_only_the_domain() {
        assert_eq!(
            parse("https://user:pw@login.example.com:8443/realms/master").as_deref(),
            Ok("example.com")
        );
    }

    #[test]
    fn unicode_domain_is_idna_encoded() {
        assert_eq!(parse("auth.bücher.de").as_deref(), Ok("xn--bcher-kva.de"));
    }

    #[test]
    fn private_suffix_keeps_the_tenant_label() {
        assert_eq!(parse("acme.github.io").as_deref(), Ok("acme.github.io"));
    }

    #[test]
    fn bare_public_suffix_is_rejected() {
        assert_eq!(
            parse("co.uk"),
            Err(NormalizedDomainError::NoRegistrableDomain)
        );
    }

    #[test]
    fn unknown_suffix_is_rejected() {
        assert_eq!(
            parse("intranet.corp.internal"),
            Err(NormalizedDomainError::NoRegistrableDomain)
        );
    }

    #[test]
    fn ip_addresses_are_rejected() {
        assert_eq!(parse("10.0.0.1"), Err(NormalizedDomainError::IpAddress));
        assert_eq!(
            parse("https://[2001:db8::1]/"),
            Err(NormalizedDomainError::IpAddress)
        );
    }

    #[test]
    fn invalid_inputs_are_typed_errors() {
        assert_eq!(parse(""), Err(NormalizedDomainError::Empty));
        assert_eq!(parse("   "), Err(NormalizedDomainError::Empty));
        assert_eq!(
            parse("exa mple.com"),
            Err(NormalizedDomainError::InvalidHost)
        );
        assert_eq!(parse("http://"), Err(NormalizedDomainError::InvalidUrl));
        assert_eq!(
            parse("mailto:a@example.com"),
            Err(NormalizedDomainError::InvalidHost)
        );
    }

    #[test]
    fn overlong_host_is_rejected() {
        let host = format!("{}.example.com", vec!["a".repeat(60); 5].join("."));
        assert_eq!(parse(&host), Err(NormalizedDomainError::InvalidHost));
    }

    fn hostname() -> impl Strategy<Value = String> {
        (
            proptest::collection::vec("[a-z0-9]{1,12}", 0..4),
            "[a-z0-9]{1,12}",
            prop_oneof![
                Just("com"),
                Just("fr"),
                Just("co.uk"),
                Just("de"),
                Just("io")
            ],
        )
            .prop_map(|(subs, name, suffix)| {
                let mut labels = subs;
                labels.push(name);
                labels.push(suffix.to_owned());
                labels.join(".")
            })
    }

    proptest! {
        #[test]
        fn normalization_is_idempotent(host in hostname()) {
            let once = parse(&host).unwrap();
            prop_assert_eq!(parse(&once).unwrap(), once);
        }

        #[test]
        fn normalization_ignores_case(host in hostname()) {
            prop_assert_eq!(parse(&host.to_uppercase()), parse(&host));
        }

        #[test]
        fn url_and_host_forms_agree(host in hostname(), path in "[a-z0-9/]{0,12}") {
            let url = format!("https://{host}/{path}");
            prop_assert_eq!(parse(&url), parse(&host));
        }

        #[test]
        fn arbitrary_input_never_panics(input in ".{0,300}") {
            let _ = parse(&input);
        }
    }
}
