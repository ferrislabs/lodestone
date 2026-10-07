pub mod catalog;
pub mod configuration;
pub mod descriptor;
pub mod oauth;
pub mod ports;
pub mod probe;
pub mod validation;

pub use catalog::{catalog, descriptor};
pub use configuration::{
    ConnectedAccount, ProviderConfiguration, ProviderStatus, TOKEN_ACCESS, TOKEN_REFRESH,
    mask_secret, secret_context,
};
pub use descriptor::{
    AuthKind, AuthSpec, Capabilities, FieldKind, FieldSpec, FilterSupport, ProviderCategory,
    SourceProviderDescriptor,
};
pub use oauth::{OAUTH_STATE_TTL, OAuthState};
pub use ports::SourceProviderRepository;
pub use probe::{Credential, ProbeContext, ProbeOutcome, ProviderProbe};
pub use validation::{FieldProblem, missing_secrets, validate_settings};
