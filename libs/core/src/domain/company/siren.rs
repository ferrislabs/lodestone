use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Error)]
pub enum InvalidSiren {
    #[error("a SIREN is 9 digits, a SIRET 14")]
    Shape,
    #[error("the check digit of the SIREN is wrong")]
    Checksum,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Siren(Box<str>);

fn luhn_valid(digits: &str) -> bool {
    let sum: u32 = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let value = u32::from(byte - b'0');
            if index % 2 == 1 {
                let doubled = value * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                value
            }
        })
        .sum();
    sum.is_multiple_of(10)
}

impl Siren {
    pub fn parse(raw: &str) -> Result<Self, InvalidSiren> {
        let digits: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
        if !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidSiren::Shape);
        }
        let siren = match digits.len() {
            9 => digits.as_str(),
            14 => &digits[..9],
            _ => return Err(InvalidSiren::Shape),
        };
        if luhn_valid(siren) {
            Ok(Self(siren.into()))
        } else {
            Err(InvalidSiren::Checksum)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Siren {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Siren {
    type Error = InvalidSiren;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<Siren> for String {
    fn from(value: Siren) -> Self {
        value.0.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_siren_is_accepted() {
        assert_eq!(Siren::parse("552081317").unwrap().as_str(), "552081317");
        assert_eq!(Siren::parse("552 081 317").unwrap().as_str(), "552081317");
    }

    #[test]
    fn a_siret_gives_its_first_nine_digits() {
        assert_eq!(
            Siren::parse("55208131766522").unwrap().as_str(),
            "552081317"
        );
    }

    #[test]
    fn a_wrong_check_digit_is_rejected() {
        assert_eq!(Siren::parse("552081318"), Err(InvalidSiren::Checksum));
        assert_eq!(Siren::parse("55208131866522"), Err(InvalidSiren::Checksum));
    }

    #[test]
    fn a_wrong_shape_is_rejected() {
        for raw in [
            "",
            "55208131",
            "5520813170",
            "55208131766522 0",
            "55208131a",
        ] {
            assert_eq!(Siren::parse(raw), Err(InvalidSiren::Shape), "{raw}");
        }
    }

    #[test]
    fn it_round_trips_through_json_and_validates_on_read() {
        let siren = Siren::parse("552081317").unwrap();
        let json = serde_json::to_string(&siren).unwrap();
        assert_eq!(json, "\"552081317\"");
        assert_eq!(serde_json::from_str::<Siren>(&json).unwrap(), siren);
        assert!(serde_json::from_str::<Siren>("\"552081318\"").is_err());
    }
}
