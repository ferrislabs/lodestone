use std::{fmt, str::FromStr};

use thiserror::Error;
use url::Url;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValueError {
    #[error("value must be a finite number between 0 and 1")]
    OutOfUnitInterval,
    #[error("provenance must be an absolute http or https URL")]
    InvalidProvenance,
    #[error("email address is malformed")]
    InvalidEmail,
    #[error("signal kind must be lowercase snake_case")]
    InvalidSignalKind,
    #[error("unknown source `{0}`")]
    UnknownSource(String),
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UnitInterval(f64);

impl UnitInterval {
    pub fn new(value: f64) -> Result<Self, ValueError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValueError::OutOfUnitInterval)
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance(Url);

impl Provenance {
    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let url = Url::parse(raw).map_err(|_| ValueError::InvalidProvenance)?;
        match url.scheme() {
            "http" | "https" if url.host().is_some() => Ok(Self(url)),
            _ => Err(ValueError::InvalidProvenance),
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmailAddress(Box<str>);

impl EmailAddress {
    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let lowered = raw.trim().to_ascii_lowercase();
        let (local, domain) = lowered.split_once('@').ok_or(ValueError::InvalidEmail)?;
        let valid = !local.is_empty()
            && !domain.contains('@')
            && domain.contains('.')
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !lowered.contains(char::is_whitespace);
        if valid {
            Ok(Self(lowered.into()))
        } else {
            Err(ValueError::InvalidEmail)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn local_part(&self) -> &str {
        self.0.split_once('@').map_or("", |(local, _)| local)
    }

    pub fn domain(&self) -> &str {
        self.0.split_once('@').map_or("", |(_, domain)| domain)
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalKind(Box<str>);

impl SignalKind {
    pub fn parse(raw: &str) -> Result<Self, ValueError> {
        let valid = !raw.is_empty()
            && raw.len() <= 64
            && raw.starts_with(|c: char| c.is_ascii_lowercase())
            && raw
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if valid {
            Ok(Self(raw.into()))
        } else {
            Err(ValueError::InvalidSignalKind)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceName {
    CertificateTransparency,
    HttpProbe,
    DnsCname,
    PublicPage,
    SecurityTxt,
    Boamp,
    Decp,
    Ted,
    FranceTravail,
    Ats,
}

impl SourceName {
    pub const ALL: [Self; 10] = [
        Self::CertificateTransparency,
        Self::HttpProbe,
        Self::DnsCname,
        Self::PublicPage,
        Self::SecurityTxt,
        Self::Boamp,
        Self::Decp,
        Self::Ted,
        Self::FranceTravail,
        Self::Ats,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::CertificateTransparency => "certificate_transparency",
            Self::HttpProbe => "http_probe",
            Self::DnsCname => "dns_cname",
            Self::PublicPage => "public_page",
            Self::SecurityTxt => "security_txt",
            Self::Boamp => "boamp",
            Self::Decp => "decp",
            Self::Ted => "ted",
            Self::FranceTravail => "france_travail",
            Self::Ats => "ats",
        }
    }
}

impl FromStr for SourceName {
    type Err = ValueError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|source| source.as_str() == raw)
            .ok_or_else(|| ValueError::UnknownSource(raw.to_owned()))
    }
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_interval_bounds() {
        assert!(UnitInterval::new(0.0).is_ok());
        assert!(UnitInterval::new(1.0).is_ok());
        assert_eq!(UnitInterval::new(1.01), Err(ValueError::OutOfUnitInterval));
        assert_eq!(UnitInterval::new(-0.1), Err(ValueError::OutOfUnitInterval));
        assert_eq!(
            UnitInterval::new(f64::NAN),
            Err(ValueError::OutOfUnitInterval)
        );
    }

    #[test]
    fn provenance_accepts_only_web_urls() {
        assert!(Provenance::parse("https://example.com/legal").is_ok());
        assert!(Provenance::parse("http://example.com").is_ok());
        assert_eq!(
            Provenance::parse("file:///etc/passwd"),
            Err(ValueError::InvalidProvenance)
        );
        assert_eq!(
            Provenance::parse("example.com"),
            Err(ValueError::InvalidProvenance)
        );
    }

    #[test]
    fn email_is_lowercased_and_split() {
        let email = EmailAddress::parse(" Security@Example.COM ").unwrap();
        assert_eq!(email.as_str(), "security@example.com");
        assert_eq!(email.local_part(), "security");
        assert_eq!(email.domain(), "example.com");
    }

    #[test]
    fn malformed_emails_are_rejected() {
        for raw in [
            "",
            "a",
            "a@b",
            "@b.com",
            "a@@b.com",
            "a b@c.com",
            "a@.com",
            "a@b.com.",
        ] {
            assert_eq!(
                EmailAddress::parse(raw),
                Err(ValueError::InvalidEmail),
                "{raw}"
            );
        }
    }

    #[test]
    fn signal_kind_is_snake_case() {
        assert!(SignalKind::parse("product_detected").is_ok());
        assert_eq!(
            SignalKind::parse("Product"),
            Err(ValueError::InvalidSignalKind)
        );
        assert_eq!(SignalKind::parse("_x"), Err(ValueError::InvalidSignalKind));
        assert_eq!(SignalKind::parse(""), Err(ValueError::InvalidSignalKind));
    }

    #[test]
    fn source_names_round_trip() {
        for source in SourceName::ALL {
            assert_eq!(source.as_str().parse::<SourceName>(), Ok(source));
        }
        assert!("linkedin".parse::<SourceName>().is_err());
    }
}
