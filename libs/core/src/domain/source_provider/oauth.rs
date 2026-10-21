use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::domain::{llm::provider::SealedSecret, user::UserId};

pub const OAUTH_STATE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthState {
    pub state: String,
    pub provider_key: String,
    pub user_id: UserId,
    pub code_verifier: SealedSecret,
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl OAuthState {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }
}
