use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileDefinition {
    pub schema_version: u32,
    pub identity: Identity,
    pub sources: Sources,
    pub scoring: Scoring,
    pub contact_ranking: ContactRanking,
    pub llm: LlmSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub offer: String,
    pub target_customer: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_transparency: Option<CertificateTransparency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_probe: Option<HttpProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_cname: Option<DnsCname>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boamp: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decp: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ted: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub france_travail: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ats: Option<KeywordSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CertificateTransparency {
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpProbe {
    pub fingerprints: Vec<Fingerprint>,
    #[serde(default)]
    pub excluded_products: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    pub id: String,
    pub path: String,
    pub product: String,
    pub matchers: Vec<Matcher>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionExtractor>,
    #[serde(default)]
    pub debt_markers: Vec<DebtMarker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Matcher {
    Status { equals: u16 },
    Header { name: String, pattern: String },
    Body { pattern: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "snake_case", deny_unknown_fields)]
pub enum VersionExtractor {
    Header { name: String, pattern: String },
    Body { pattern: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DebtMarker {
    pub id: String,
    pub rule: DebtRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DebtRule {
    Matches { matcher: Matcher },
    VersionBelow { version: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DnsCname {
    pub patterns: Vec<CnamePattern>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CnamePattern {
    pub provider: String,
    pub target_suffix: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeywordSource {
    pub keywords: Vec<String>,
    #[serde(default)]
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scoring {
    pub factors: Vec<ScoringFactor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoringFactor {
    pub signal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactRanking {
    pub preferred_local_parts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LlmSettings {
    pub qualification_threshold: f64,
    pub relevance_criteria: String,
}
