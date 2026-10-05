use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use thiserror::Error;

use crate::domain::{lead::uuid_id, profile::ProfileId, values::Provenance};

pub mod ports;

uuid_id!(PartnerId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
#[error("a SIRET is exactly 14 ASCII digits")]
pub struct InvalidSiret;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Siret(Box<str>);

impl Siret {
    pub fn parse(raw: &str) -> Result<Self, InvalidSiret> {
        let raw = raw.trim();
        if raw.len() == 14 && raw.bytes().all(|byte| byte.is_ascii_digit()) {
            Ok(Self(raw.into()))
        } else {
            Err(InvalidSiret)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Siret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartnerAward {
    pub holder: Siret,
    pub holder_name: Option<String>,
    pub contract_id: String,
    pub buyer: Siret,
    pub object: String,
    pub amount: Option<f64>,
    pub notified_on: NaiveDate,
    pub provenance: Provenance,
    pub matched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partner {
    pub id: PartnerId,
    pub profile_id: ProfileId,
    pub siret: Siret,
    pub name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_award_at: Option<NaiveDate>,
    pub award_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourteen_digits_are_accepted() {
        assert_eq!(
            Siret::parse("25351449100047").unwrap().as_str(),
            "25351449100047"
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        assert_eq!(
            Siret::parse(" 25351449100047\n").unwrap().as_str(),
            "25351449100047"
        );
    }

    #[test]
    fn wrong_length_is_rejected() {
        assert!(Siret::parse("2535144910004").is_err());
        assert!(Siret::parse("253514491000470").is_err());
        assert!(Siret::parse("").is_err());
    }

    #[test]
    fn non_digits_are_rejected() {
        assert!(Siret::parse("2535144910004X").is_err());
        assert!(Siret::parse("2535144910004٣").is_err());
        assert!(Siret::parse("253 514 491 0004").is_err());
    }
}
