use serde_json::{Map, Value};

use crate::domain::lead::str_enum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderCategory {
    PublicProcurement,
    JobsAndMissions,
    TechnicalSignals,
    Companies,
}

str_enum!(ProviderCategory {
    PublicProcurement => "public_procurement",
    JobsAndMissions => "jobs_and_missions",
    TechnicalSignals => "technical_signals",
    Companies => "companies",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthKind {
    None,
    ApiKey,
    OAuth2ClientCredentials,
    OAuth2AuthorizationCode,
}

str_enum!(AuthKind {
    None => "none",
    ApiKey => "api_key",
    OAuth2ClientCredentials => "oauth2_client_credentials",
    OAuth2AuthorizationCode => "oauth2_authorization_code",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSpec {
    None,
    ApiKey {
        header: &'static str,
        prefix: &'static str,
    },
    OAuth2ClientCredentials {
        token_url: &'static str,
        scopes: &'static [&'static str],
    },
    OAuth2AuthorizationCode {
        authorize_url: &'static str,
        token_url: &'static str,
        revoke_url: Option<&'static str>,
        userinfo_url: Option<&'static str>,
        scopes: &'static [&'static str],
        pkce: bool,
    },
}

impl AuthSpec {
    pub fn kind(&self) -> AuthKind {
        match self {
            Self::None => AuthKind::None,
            Self::ApiKey { .. } => AuthKind::ApiKey,
            Self::OAuth2ClientCredentials { .. } => AuthKind::OAuth2ClientCredentials,
            Self::OAuth2AuthorizationCode { .. } => AuthKind::OAuth2AuthorizationCode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Url,
    Secret,
    Select,
    Number,
    Bool,
}

str_enum!(FieldKind {
    Text => "text",
    Url => "url",
    Secret => "secret",
    Select => "select",
    Number => "number",
    Bool => "bool",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub default: Option<&'static str>,
    pub help: &'static str,
    pub options: &'static [&'static str],
    pub range: Option<(i64, i64)>,
    pub hosts: &'static [&'static str],
}

impl FieldSpec {
    pub const fn text(key: &'static str, label: &'static str, required: bool) -> Self {
        Self::of(key, label, FieldKind::Text, required)
    }

    pub const fn url(key: &'static str, label: &'static str, required: bool) -> Self {
        Self::of(key, label, FieldKind::Url, required)
    }

    pub const fn secret(key: &'static str, label: &'static str, required: bool) -> Self {
        Self::of(key, label, FieldKind::Secret, required)
    }

    const fn of(key: &'static str, label: &'static str, kind: FieldKind, required: bool) -> Self {
        Self {
            key,
            label,
            kind,
            required,
            default: None,
            help: "",
            options: &[],
            range: None,
            hosts: &[],
        }
    }

    pub const fn flag(key: &'static str, label: &'static str, required: bool) -> Self {
        Self::of(key, label, FieldKind::Bool, required)
    }

    pub const fn number(key: &'static str, label: &'static str, required: bool) -> Self {
        Self::of(key, label, FieldKind::Number, required)
    }

    pub const fn with_default(mut self, default: &'static str) -> Self {
        self.default = Some(default);
        self
    }

    pub const fn with_help(mut self, help: &'static str) -> Self {
        self.help = help;
        self
    }

    pub const fn with_options(mut self, options: &'static [&'static str]) -> Self {
        self.options = options;
        self
    }

    pub const fn with_range(mut self, min: i64, max: i64) -> Self {
        self.range = Some((min, max));
        self
    }

    pub const fn with_hosts(mut self, hosts: &'static [&'static str]) -> Self {
        self.hosts = hosts;
        self
    }

    pub fn is_attestation(&self) -> bool {
        self.kind == FieldKind::Bool && self.required
    }

    pub fn accepts_host(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.hosts.is_empty()
            || self.hosts.iter().any(|allowed| {
                host == *allowed
                    || host
                        .strip_suffix(allowed)
                        .is_some_and(|rest| rest.ends_with('.'))
            })
    }

    pub fn is_secret(&self) -> bool {
        self.kind == FieldKind::Secret
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilterSupport {
    pub remote: bool,
    pub countries: bool,
    pub contract: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub keyword_search: bool,
    pub filters: FilterSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceProviderDescriptor {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: ProviderCategory,
    pub auth: AuthSpec,
    pub fields: &'static [FieldSpec],
    pub capabilities: Capabilities,
    pub terms_note: Option<&'static str>,
}

impl SourceProviderDescriptor {
    pub fn field(&self, key: &str) -> Option<&'static FieldSpec> {
        self.fields.iter().find(|field| field.key == key)
    }

    pub fn secret_fields(&self) -> impl Iterator<Item = &'static FieldSpec> {
        self.fields.iter().filter(|field| field.is_secret())
    }

    pub fn setting_fields(&self) -> impl Iterator<Item = &'static FieldSpec> {
        self.fields.iter().filter(|field| !field.is_secret())
    }

    pub fn endpoint<'a>(
        &'a self,
        settings: &'a Map<String, Value>,
        key: &str,
        default: &'a str,
    ) -> &'a str {
        settings
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(default)
    }
}
