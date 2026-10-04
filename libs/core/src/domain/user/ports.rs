use common::CoreError;

use crate::User;

#[cfg_attr(test, mockall::automock)]
pub trait UserRepository: Send {
    fn upsert_by_sub(
        &mut self,
        user: &User,
    ) -> impl Future<Output = Result<User, CoreError>> + Send;
}
