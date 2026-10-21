pub mod catalog;
pub mod configuration;
pub mod descriptor;
pub mod oauth;
pub mod ports;
pub mod probe;
pub mod validation;

pub use catalog::{
    ATS, BOAMP, BODACC, BODACC_DEFAULT_URL, CERTIFICATE_TRANSPARENCY, COMPANY_RESEARCH, CONTACTS,
    DECP, DECP_HOLDERS_DEFAULT_URL, DNS_CNAME, FRANCE_TRAVAIL, HACKER_NEWS_HIRING,
    HACKER_NEWS_THREAD_TITLE, HTTP_PROBE, TED, WELCOME_TO_THE_JUNGLE, WELCOME_TO_THE_JUNGLE_HOST,
    catalog, descriptor, provider_key_of_source,
};
pub use configuration::{
    ConnectedAccount, ProviderConfiguration, ProviderStatus, TOKEN_ACCESS, TOKEN_REFRESH,
    mask_secret, secret_context, stamp_attestations,
};
pub use descriptor::{
    AuthKind, AuthSpec, Capabilities, FieldKind, FieldSpec, FilterSupport, ProviderCategory,
    SourceProviderDescriptor,
};
pub use oauth::{OAUTH_STATE_TTL, OAuthState};
pub use ports::SourceProviderRepository;
pub use probe::{Credential, ProbeContext, ProbeOutcome, ProviderProbe};
pub use validation::{FieldProblem, missing_secrets, validate_settings};
