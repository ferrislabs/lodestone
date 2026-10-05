use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Detection profile",
    description = "Describes one market: what is sold, which sources to scan, how leads are scored, which contacts to prefer and how the LLM qualifies a lead."
)]
pub struct ProfileDefinition {
    #[schemars(description = "Profile format version. Must be 1.")]
    pub schema_version: u32,
    pub identity: Identity,
    pub sources: Sources,
    pub scoring: Scoring,
    pub contact_ranking: ContactRanking,
    pub llm: LlmSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Identity",
    description = "The offer and the target customer, in free text."
)]
pub struct Identity {
    #[schemars(description = "What is sold, in free text.")]
    pub offer: String,
    #[schemars(description = "Who buys it, in free text.")]
    pub target_customer: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Sources",
    description = "The enabled sources. At least one is required."
)]
pub struct Sources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Hostname labels to look for in certificate logs.")]
    pub certificate_transparency: Option<CertificateTransparency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Fingerprints probed over HTTP.")]
    pub http_probe: Option<HttpProbe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Providers identified by CNAME target suffix.")]
    pub dns_cname: Option<DnsCname>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "BOAMP public tenders.")]
    pub boamp: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "DECP public procurement data.")]
    pub decp: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "TED European tenders.")]
    pub ted: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "France Travail job offers.")]
    pub france_travail: Option<KeywordSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Applicant tracking system job boards.")]
    pub ats: Option<KeywordSource>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Certificate transparency",
    description = "Hostname labels to look for in certificate logs."
)]
pub struct CertificateTransparency {
    #[schemars(description = "Hostname labels to look for, for example auth or sso.")]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "HTTP probe",
    description = "Fingerprints probed over HTTP and products to ignore."
)]
pub struct HttpProbe {
    #[schemars(description = "Fingerprints to probe on each domain.")]
    pub fingerprints: Vec<Fingerprint>,
    #[serde(default)]
    #[schemars(description = "Products to ignore, for example your own.")]
    pub excluded_products: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Fingerprint",
    description = "A product fingerprint: path to request, matchers, optional version extractor and debt markers."
)]
pub struct Fingerprint {
    #[schemars(description = "Unique identifier.")]
    pub id: String,
    #[schemars(description = "URL path to request, for example /login.")]
    pub path: String,
    #[schemars(description = "Product name raised as a signal.")]
    pub product: String,
    #[schemars(description = "Conditions on the response. All must hold.")]
    pub matchers: Vec<Matcher>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Optional extractor for the product version.")]
    pub version: Option<VersionExtractor>,
    #[serde(default)]
    #[schemars(description = "Technical debt signals for this product.")]
    pub debt_markers: Vec<DebtMarker>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(title = "Matcher", description = "A condition on the HTTP response.")]
pub enum Matcher {
    #[schemars(title = "Status")]
    Status {
        #[schemars(description = "Expected HTTP status code.")]
        equals: u16,
    },
    #[schemars(title = "Header")]
    Header {
        #[schemars(description = "Header name.")]
        name: String,
        #[schemars(description = "Regular expression matched against the header value.")]
        pattern: String,
    },
    #[schemars(title = "Body")]
    Body {
        #[schemars(description = "Regular expression matched against the body.")]
        pattern: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(
    title = "Version extractor",
    description = "Extracts a product version from the response. The pattern needs a capture group."
)]
pub enum VersionExtractor {
    #[schemars(title = "Header")]
    Header {
        #[schemars(description = "Header name.")]
        name: String,
        #[schemars(description = "Regular expression with a capture group for the version.")]
        pattern: String,
    },
    #[schemars(title = "Body")]
    Body {
        #[schemars(description = "Regular expression with a capture group for the version.")]
        pattern: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Debt marker",
    description = "A technical debt signal raised when its rule holds."
)]
pub struct DebtMarker {
    #[schemars(description = "Unique identifier.")]
    pub id: String,
    #[schemars(description = "Condition raising the marker.")]
    pub rule: DebtRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[schemars(title = "Debt rule", description = "Condition raising a debt marker.")]
pub enum DebtRule {
    #[schemars(title = "Matches")]
    Matches {
        #[schemars(description = "Response condition that must hold.")]
        matcher: Matcher,
    },
    #[schemars(title = "Version below")]
    VersionBelow {
        #[schemars(description = "Raised when the extracted version is lower than this one.")]
        version: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "DNS CNAME",
    description = "Providers identified by the CNAME target of a domain."
)]
pub struct DnsCname {
    #[schemars(description = "Provider and target suffix pairs.")]
    pub patterns: Vec<CnamePattern>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "CNAME pattern",
    description = "A provider name with the CNAME target suffix that identifies it."
)]
pub struct CnamePattern {
    #[schemars(description = "Provider name.")]
    pub provider: String,
    #[schemars(description = "CNAME target suffix identifying the provider.")]
    pub target_suffix: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Keyword source",
    description = "Source searched with keywords and exclusions."
)]
pub struct KeywordSource {
    #[schemars(description = "Keywords to search for.")]
    pub keywords: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Keywords that send a candidate to LLM triage when present.")]
    pub exclusions: Vec<String>,
    #[serde(default = "default_accept_min_keywords")]
    #[schemars(
        description = "Distinct keywords needed to accept a candidate without the LLM. Fewer, or any exclusion, sends it to LLM triage."
    )]
    pub accept_min_keywords: u8,
}

fn default_accept_min_keywords() -> u8 {
    2
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Scoring",
    description = "Weighted factors. A lead score is the sum of weight times measure over the factors."
)]
pub struct Scoring {
    #[schemars(description = "Weighted factors.")]
    pub factors: Vec<ScoringFactor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Scoring factor", description = "One weighted signal kind.")]
pub struct ScoringFactor {
    #[schemars(
        description = "Signal kind. The reserved distinct_signals measures how many distinct signal kinds a lead has."
    )]
    pub signal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "Signal value to match. Any value when absent.")]
    pub value: Option<String>,
    #[schemars(description = "Weight applied to the measure of the signal.")]
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Contact ranking",
    description = "Preferred email local parts, most preferred first."
)]
pub struct ContactRanking {
    #[schemars(description = "Preferred email local parts, most preferred first.")]
    pub preferred_local_parts: Vec<String>,
    #[serde(default)]
    #[schemars(description = "Local parts to guess when no address is found.")]
    pub guess_local_parts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "LLM settings",
    description = "Qualification threshold and relevance criteria given to the model."
)]
pub struct LlmSettings {
    #[schemars(description = "Score from which a lead is qualified.")]
    pub qualification_threshold: f64,
    #[schemars(description = "Criteria given to the LLM to judge a lead relevance.")]
    pub relevance_criteria: String,
}
