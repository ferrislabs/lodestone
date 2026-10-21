use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::domain::values::SourceName;

pub const CONTINUOUS_SOURCES: [SourceName; 6] = [
    SourceName::CertificateTransparency,
    SourceName::DnsCname,
    SourceName::HttpProbe,
    SourceName::SecurityTxt,
    SourceName::PublicPage,
    SourceName::RoleGuess,
];

pub const STALE_FLOOR: Duration = Duration::from_secs(120);

pub fn stale_after(idle: Duration) -> Duration {
    idle.saturating_mul(3).max(STALE_FLOOR)
}

pub fn is_continuous(source: SourceName) -> bool {
    CONTINUOUS_SOURCES.contains(&source)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerState {
    Standby,
    Listening,
    Idle,
    IdleNoAgent,
    Error,
    Stopped,
    Stale,
}

impl ListenerState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standby => "standby",
            Self::Listening => "listening",
            Self::Idle => "idle",
            Self::IdleNoAgent => "idle_no_agent",
            Self::Error => "error",
            Self::Stopped => "stopped",
            Self::Stale => "stale",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        [
            Self::Standby,
            Self::Listening,
            Self::Idle,
            Self::IdleNoAgent,
            Self::Error,
            Self::Stopped,
            Self::Stale,
        ]
        .into_iter()
        .find(|state| state.as_str() == raw)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepOutcome {
    pub processed: u64,
    pub signals: u64,
    pub backlog: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerStatus {
    pub name: String,
    pub holder: Option<String>,
    pub state: ListenerState,
    pub last_activity_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub last_error_at: Option<DateTime<Utc>>,
    pub items_total: u64,
    pub signals_total: u64,
    pub backlog: Option<u64>,
    pub updated_at: DateTime<Utc>,
}

impl ListenerStatus {
    pub fn observed(mut self, now: DateTime<Utc>, stale_after: Duration) -> Self {
        let silent = chrono::Duration::from_std(stale_after)
            .is_ok_and(|limit| now - self.updated_at > limit);
        let beats = !matches!(self.state, ListenerState::Standby | ListenerState::Stopped);
        if silent && beats {
            self.state = ListenerState::Stale;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusUpdate<'a> {
    pub name: &'a str,
    pub holder: &'a str,
    pub state: ListenerState,
    pub outcome: StepOutcome,
    pub error: Option<&'a str>,
    pub succeeded: bool,
    pub at: DateTime<Utc>,
}

pub fn lock_key(name: &str) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in name.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    i64::from_ne_bytes(hash.to_ne_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip() {
        for state in [
            ListenerState::Standby,
            ListenerState::Listening,
            ListenerState::Idle,
            ListenerState::IdleNoAgent,
            ListenerState::Error,
            ListenerState::Stopped,
            ListenerState::Stale,
        ] {
            assert_eq!(ListenerState::parse(state.as_str()), Some(state));
        }
        assert_eq!(ListenerState::parse("nope"), None);
    }

    fn status(state: ListenerState, updated_secs_ago: i64) -> ListenerStatus {
        ListenerStatus {
            name: "dns_cname".to_owned(),
            holder: Some("a".to_owned()),
            state,
            last_activity_at: None,
            last_error: None,
            last_error_at: None,
            items_total: 0,
            signals_total: 0,
            backlog: None,
            updated_at: Utc::now() - chrono::Duration::seconds(updated_secs_ago),
        }
    }

    #[test]
    fn the_stale_threshold_is_three_idle_periods_with_a_floor() {
        assert_eq!(
            stale_after(Duration::from_secs(15)),
            Duration::from_secs(120)
        );
        assert_eq!(
            stale_after(Duration::from_secs(60)),
            Duration::from_secs(180)
        );
    }

    #[test]
    fn a_silent_holder_is_reported_stale() {
        let limit = Duration::from_secs(120);
        let now = Utc::now();
        assert_eq!(
            status(ListenerState::Listening, 300)
                .observed(now, limit)
                .state,
            ListenerState::Stale
        );
        assert_eq!(
            status(ListenerState::Error, 300).observed(now, limit).state,
            ListenerState::Stale
        );
        assert_eq!(
            status(ListenerState::Listening, 30)
                .observed(now, limit)
                .state,
            ListenerState::Listening
        );
        assert_eq!(
            status(ListenerState::Stopped, 300)
                .observed(now, limit)
                .state,
            ListenerState::Stopped
        );
        assert_eq!(
            status(ListenerState::Standby, 300)
                .observed(now, limit)
                .state,
            ListenerState::Standby
        );
    }

    #[test]
    fn lock_keys_are_stable_and_distinct() {
        let keys: Vec<i64> = CONTINUOUS_SOURCES
            .iter()
            .map(|source| lock_key(source.as_str()))
            .collect();
        for (index, key) in keys.iter().enumerate() {
            assert!(!keys[index + 1..].contains(key));
        }
        assert_eq!(lock_key("dns_cname"), lock_key("dns_cname"));
    }

    #[test]
    fn only_streaming_sources_are_continuous() {
        assert!(is_continuous(SourceName::CertificateTransparency));
        assert!(is_continuous(SourceName::RoleGuess));
        assert!(!is_continuous(SourceName::Boamp));
    }
}
