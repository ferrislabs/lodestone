use chrono::{DateTime, Utc};
use common::RobotsPolicy;

use crate::domain::{UserId, lead::str_enum};

pub mod ports;

pub const ROBOTS_POLICY_KEY: &str = "crawl.robots_policy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSetting {
    pub value: String,
    pub updated_by: Option<UserId>,
    pub updated_by_name: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingOrigin {
    Environment,
    Organisation,
}

str_enum!(SettingOrigin {
    Environment => "environment",
    Organisation => "organisation",
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrawlSettings {
    pub robots_policy: RobotsPolicy,
    pub origin: SettingOrigin,
    pub updated_by: Option<UserId>,
    pub updated_by_name: Option<String>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl CrawlSettings {
    pub fn effective(stored: Option<StoredSetting>, seed: RobotsPolicy) -> Self {
        match stored
            .and_then(|stored| RobotsPolicy::parse(&stored.value).map(|policy| (policy, stored)))
        {
            Some((policy, stored)) => Self {
                robots_policy: policy,
                origin: SettingOrigin::Organisation,
                updated_by: stored.updated_by,
                updated_by_name: stored.updated_by_name,
                updated_at: Some(stored.updated_at),
            },
            None => Self {
                robots_policy: seed,
                origin: SettingOrigin::Environment,
                updated_by: None,
                updated_by_name: None,
                updated_at: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;

    fn stored(value: &str) -> StoredSetting {
        StoredSetting {
            value: value.to_owned(),
            updated_by: Some(UserId(Uuid::nil())),
            updated_by_name: Some("Alice".to_owned()),
            updated_at: Utc.timestamp_opt(1_000, 0).unwrap(),
        }
    }

    #[test]
    fn the_seed_applies_until_a_value_is_saved() {
        let settings = CrawlSettings::effective(None, RobotsPolicy::Ignore);
        assert_eq!(settings.robots_policy, RobotsPolicy::Ignore);
        assert_eq!(settings.origin, SettingOrigin::Environment);
        assert!(settings.updated_at.is_none());
    }

    #[test]
    fn a_saved_value_wins_over_the_seed() {
        let settings = CrawlSettings::effective(Some(stored("respect")), RobotsPolicy::Ignore);
        assert_eq!(settings.robots_policy, RobotsPolicy::Respect);
        assert_eq!(settings.origin, SettingOrigin::Organisation);
        assert_eq!(settings.updated_by_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn an_unreadable_saved_value_falls_back_to_the_seed() {
        let settings = CrawlSettings::effective(Some(stored("maybe")), RobotsPolicy::Respect);
        assert_eq!(settings.robots_policy, RobotsPolicy::Respect);
        assert_eq!(settings.origin, SettingOrigin::Environment);
    }
}
