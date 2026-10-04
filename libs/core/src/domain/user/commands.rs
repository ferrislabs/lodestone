#[derive(Debug, Clone)]
pub struct CreateUserCommand {
    pub name: String,
    pub username: String,
    pub email: Option<String>,
    pub sub: String,
}
