use std::{fmt, num::NonZeroU32, str::FromStr};

use chrono::{DateTime, Utc};
use serde::Serialize;
use thiserror::Error;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::UserId;

pub mod commands;
pub mod definition;
mod legacy;
pub mod ports;
pub mod service;
pub mod validation;

pub use validation::{ProfileProblem, ProfileValidationError, ValidatedProfile};

const MAX_PROFILE_NAME_LENGTH: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, ToSchema)]
pub struct ProfileId(pub Uuid);

impl FromStr for ProfileId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(ProfileId)
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProfileNameError {
    #[error("profile name must not be empty")]
    Empty,
    #[error("profile name must be at most {MAX_PROFILE_NAME_LENGTH} characters")]
    TooLong,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProfileName(Box<str>);

impl ProfileName {
    pub fn parse(raw: &str) -> Result<Self, ProfileNameError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ProfileNameError::Empty);
        }
        if trimmed.chars().count() > MAX_PROFILE_NAME_LENGTH {
            return Err(ProfileNameError::TooLong);
        }
        Ok(Self(trimmed.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionNumber(NonZeroU32);

impl RevisionNumber {
    pub const FIRST: Self = Self(NonZeroU32::MIN);

    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }

    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub id: ProfileId,
    pub name: ProfileName,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Revised {
    Created(ProfileRevision),
    Unchanged(ProfileRevision),
}

impl Revised {
    pub fn revision(&self) -> &ProfileRevision {
        match self {
            Self::Created(revision) | Self::Unchanged(revision) => revision,
        }
    }

    pub fn is_created(&self) -> bool {
        matches!(self, Self::Created(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRevision {
    pub profile_id: ProfileId,
    pub number: RevisionNumber,
    pub definition: ValidatedProfile,
    pub created_by: Option<UserId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveProfile {
    pub profile: Profile,
    pub revision: ProfileRevision,
}

#[cfg(test)]
pub(crate) mod fixtures {
    pub const MINIMAL_PROFILE: &str = r#"{
        "schema_version": 2,
        "identity": {"offer": "Managed IAM", "target_customer": "Companies running Keycloak"},
        "agents": [{
            "id": "http-probe",
            "name": "HTTP probe",
            "source": {
                "kind": "http_probe",
                "fingerprints": [{
                    "id": "keycloak-master-realm",
                    "path": "/realms/master",
                    "product": "keycloak",
                    "matchers": [
                        {"kind": "status", "equals": 200},
                        {"kind": "body", "pattern": "\"realm\"\\s*:\\s*\"master\""}
                    ],
                    "version": {"from": "header", "name": "x-keycloak-version", "pattern": "^(\\d+\\.\\d+)"},
                    "debt_markers": [{"id": "old", "rule": {"kind": "version_below", "version": "22.0"}}]
                }],
                "excluded_products": ["ferriskey"]
            }
        }],
        "scoring": {"factors": [{"signal": "product_detected", "value": "keycloak", "weight": 10.0}]},
        "contact_ranking": {"preferred_local_parts": ["security", "contact"]},
        "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Self-hosts an IAM product"}
    }"#;
}
