use std::fmt;

use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::domain::{NormalizedDomain, lead::str_enum};

pub mod ports;

const MAX_HOSTNAME_LENGTH: usize = 253;
const MAX_LABEL_LENGTH: usize = 63;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("not a valid public hostname")]
pub struct InvalidHostname;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hostname(Box<str>);

impl Hostname {
    pub fn parse(raw: &str) -> Result<Self, InvalidHostname> {
        let trimmed = raw.trim();
        let trimmed = trimmed.strip_suffix('.').unwrap_or(trimmed);
        let ascii = idna::domain_to_ascii_strict(trimmed).map_err(|_| InvalidHostname)?;
        let numeric_tld = ascii
            .rsplit('.')
            .next()
            .is_some_and(|tld| tld.bytes().all(|b| b.is_ascii_digit()));
        let valid = !ascii.is_empty()
            && !numeric_tld
            && ascii.len() <= MAX_HOSTNAME_LENGTH
            && ascii.contains('.')
            && ascii.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= MAX_LABEL_LENGTH
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            });
        if valid {
            Ok(Self(ascii.into()))
        } else {
            Err(InvalidHostname)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.0.split('.')
    }

    pub fn registrable_domain(&self) -> Option<NormalizedDomain> {
        NormalizedDomain::from_host(&self.0).ok()
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostOrigin {
    Import,
    CertificateTransparency,
}

str_enum!(HostOrigin {
    Import => "import",
    CertificateTransparency => "certificate_transparency",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateHost {
    pub host: Hostname,
    pub domain: NormalizedDomain,
    pub origin: HostOrigin,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

impl CandidateHost {
    pub fn discovered(
        host: Hostname,
        origin: HostOrigin,
        at: DateTime<Utc>,
    ) -> Result<Self, InvalidHostname> {
        let domain = host.registrable_domain().ok_or(InvalidHostname)?;
        Ok(Self {
            host,
            domain,
            origin,
            first_seen_at: at,
            last_seen_at: at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_is_normalized() {
        let host = Hostname::parse("SSO.Example.COM.").unwrap();
        assert_eq!(host.as_str(), "sso.example.com");
        assert_eq!(host.labels().next(), Some("sso"));
        assert_eq!(host.registrable_domain().unwrap().as_str(), "example.com");
    }

    #[test]
    fn invalid_hostnames_are_rejected() {
        for raw in [
            "",
            "localhost",
            "a..b.com",
            "-a.example.com",
            "a b.com",
            "*.example.com",
            "10.0.0.1",
        ] {
            assert_eq!(Hostname::parse(raw), Err(InvalidHostname), "{raw}");
        }
    }

    #[test]
    fn host_without_registrable_domain_cannot_be_a_candidate() {
        let host = Hostname::parse("corp.internal").unwrap();
        assert!(CandidateHost::discovered(host, HostOrigin::Import, Utc::now()).is_err());
    }
}
