use std::{
    fmt::{Display, Formatter, Result as FmtResult},
    str::FromStr,
};

use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RobotsPolicy {
    #[default]
    Respect,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("robots policy must be `respect` or `ignore`")]
pub struct InvalidRobotsPolicy;

impl RobotsPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Respect => "respect",
            Self::Ignore => "ignore",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "respect" => Some(Self::Respect),
            "ignore" => Some(Self::Ignore),
            _ => None,
        }
    }

    pub fn ignores_robots(self) -> bool {
        self == Self::Ignore
    }
}

impl FromStr for RobotsPolicy {
    type Err = InvalidRobotsPolicy;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::parse(raw).ok_or(InvalidRobotsPolicy)
    }
}

impl Display for RobotsPolicy {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respect_is_the_default() {
        assert_eq!(RobotsPolicy::default(), RobotsPolicy::Respect);
        assert!(!RobotsPolicy::default().ignores_robots());
    }

    #[test]
    fn parses_both_values_and_rejects_anything_else() {
        assert_eq!("respect".parse(), Ok(RobotsPolicy::Respect));
        assert_eq!(" Ignore ".parse(), Ok(RobotsPolicy::Ignore));
        assert_eq!("off".parse::<RobotsPolicy>(), Err(InvalidRobotsPolicy));
        assert_eq!(RobotsPolicy::Ignore.to_string(), "ignore");
    }
}
