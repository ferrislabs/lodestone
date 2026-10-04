use common::CoreError;
use oxid_macros::transactional;

use crate::{
    User,
    application::OxidUseCase,
    domain::user::{commands::CreateUserCommand, service::UserService},
};

impl OxidUseCase {
    #[transactional(user)]
    pub async fn create_user(&self, command: CreateUserCommand) -> Result<User, CoreError> {
        let mut service = UserService::new(user_repository);
        service.create_user(command).await
    }
}
