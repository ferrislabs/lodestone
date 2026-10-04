use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    User,
    application::LodestoneUseCase,
    domain::user::{commands::CreateUserCommand, service::UserService},
};

impl LodestoneUseCase {
    #[transactional(user)]
    pub async fn create_user(&self, command: CreateUserCommand) -> Result<User, CoreError> {
        let mut service = UserService::new(user_repository);
        service.create_user(command).await
    }
}
