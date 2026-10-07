use chrono::{DateTime, Utc};
use common::CoreError;

use super::{configuration::ProviderConfiguration, oauth::OAuthState};

#[cfg_attr(test, mockall::automock)]
pub trait SourceProviderRepository: Send {
    fn find(
        &mut self,
        key: &str,
    ) -> impl Future<Output = Result<Option<ProviderConfiguration>, CoreError>> + Send;

    fn list(
        &mut self,
    ) -> impl Future<Output = Result<Vec<ProviderConfiguration>, CoreError>> + Send;

    fn save(
        &mut self,
        configuration: &ProviderConfiguration,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn delete(&mut self, key: &str) -> impl Future<Output = Result<bool, CoreError>> + Send;

    fn insert_oauth_state(
        &mut self,
        state: &OAuthState,
    ) -> impl Future<Output = Result<(), CoreError>> + Send;

    fn take_oauth_state(
        &mut self,
        state: &str,
    ) -> impl Future<Output = Result<Option<OAuthState>, CoreError>> + Send;

    fn purge_oauth_states(
        &mut self,
        before: DateTime<Utc>,
    ) -> impl Future<Output = Result<u64, CoreError>> + Send;
}
