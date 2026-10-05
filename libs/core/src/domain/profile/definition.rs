use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::values::SourceName;

pub const PROFILE_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Detection profile",
    description = "Describes one market: what is sold, which agents scan which sources, which qualification agents judge a lead and how an aggregator sums it up, how leads are scored and which contacts to prefer."
)]
pub struct ProfileDefinition {
    #[schemars(description = "Profile format version. Must be 4.")]
    pub schema_version: u32,
    pub identity: Identity,
    #[schemars(description = "Detection agents. At least one is required.")]
    pub agents: Vec<Agent>,
    #[schemars(
        description = "Qualification agents. Each one evaluates every lead of the profile from one angle. At least one is required."
    )]
    pub qualification_agents: Vec<QualificationAgent>,
    pub aggregator: Aggregator,
    #[serde(default)]
    pub verdict: VerdictThresholds,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(
    title = "Agent id",
    description = "Slug identifying an agent in the profile."
)]
pub struct AgentId(
    #[schemars(
        length(min = 1, max = 63),
        regex(pattern = "^[a-z0-9][a-z0-9-]{0,62}$")
    )]
    String,
);

impl AgentId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_slug(&self) -> bool {
        let bytes = self.0.as_bytes();
        (1..=63).contains(&bytes.len())
            && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Qualification agent",
    description = "Evaluates every lead of the profile from one angle by asking a model a list of questions."
)]
pub struct QualificationAgent {
    #[schemars(
        description = "Unique slug among all agents of the profile, sourcing and qualification, for example maturite-technique."
    )]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[serde(default)]
    #[schemars(
        with = "Option<String>",
        description = "Id of a declared LLM provider. The default provider applies when null."
    )]
    pub provider_id: Option<Uuid>,
    #[schemars(
        length(min = 1, max = 8000),
        description = "System prompt given to the model."
    )]
    pub system_prompt: String,
    #[serde(default = "default_weight")]
    #[schemars(
        range(min = 1, max = 10),
        description = "Integer weight from 1 to 10 of this agent in the lead confidence."
    )]
    pub weight: u8,
    #[schemars(
        length(min = 1),
        description = "Questions the agent answers. At least one is required."
    )]
    pub questions: Vec<Question>,
}

pub const MAX_PROMPT_CHARS: usize = 8000;
pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 10;

fn default_weight() -> u8 {
    5
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Question",
    description = "A multiple choice question a qualification agent answers about a lead."
)]
pub struct Question {
    #[schemars(
        length(min = 1, max = 63),
        regex(pattern = "^[a-z0-9][a-z0-9_-]{0,62}$"),
        description = "Identifier unique in the agent, used as the answer key."
    )]
    pub id: String,
    #[schemars(description = "The question asked to the model.")]
    pub wording: String,
    #[serde(default = "default_weight")]
    #[schemars(
        range(min = 1, max = 10),
        description = "Integer weight from 1 to 10 of this question in the agent score."
    )]
    pub weight: u8,
    #[schemars(
        length(min = 2, max = 10),
        description = "Possible answers, from 2 to 10, with unique values."
    )]
    pub options: Vec<QuestionOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Question option",
    description = "One possible answer to a question and the score it is worth."
)]
pub struct QuestionOption {
    #[schemars(
        length(min = 1, max = 63),
        regex(pattern = "^[a-z0-9][a-z0-9_-]{0,62}$"),
        description = "Value the model answers, unique in the question."
    )]
    pub value: String,
    #[schemars(description = "Display label.")]
    pub label: String,
    #[schemars(
        range(min = 0.0, max = 1.0),
        description = "Score from 0 to 1 of this answer."
    )]
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Aggregator",
    description = "Writes the summary of a lead from the answers of every qualification agent."
)]
pub struct Aggregator {
    #[serde(default)]
    #[schemars(
        with = "Option<String>",
        description = "Id of a declared LLM provider. The default provider applies when null."
    )]
    pub provider_id: Option<Uuid>,
    #[schemars(
        length(min = 1, max = 8000),
        description = "System prompt given to the model."
    )]
    pub system_prompt: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Verdict thresholds",
    description = "Percentages of the lead confidence that decide its verdict: reliable, to check or discarded."
)]
pub struct VerdictThresholds {
    #[serde(default = "default_reliable_from")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is reliable from this confidence percentage."
    )]
    pub reliable_from: u8,
    #[serde(default = "default_discard_below")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is discarded below this confidence percentage. Must be lower than reliable_from."
    )]
    pub discard_below: u8,
}

fn default_reliable_from() -> u8 {
    70
}

fn default_discard_below() -> u8 {
    30
}

impl Default for VerdictThresholds {
    fn default() -> Self {
        Self {
            reliable_from: default_reliable_from(),
            discard_below: default_discard_below(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Agent",
    description = "Searches one source with its own settings."
)]
pub struct Agent {
    #[schemars(description = "Unique slug in the profile, for example ats-en.")]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[schemars(description = "The source searched and its settings.")]
    pub source: AgentSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(description = "LLM settings specific to this agent.")]
    pub llm: Option<AgentLlm>,
}

impl Agent {
    pub fn relevance_criteria(&self) -> Option<&str> {
        self.llm
            .as_ref()
            .and_then(|llm| llm.relevance_criteria.as_deref())
            .map(str::trim)
            .filter(|criteria| !criteria.is_empty())
    }
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schemars(
    title = "Agent source",
    description = "The source an agent searches, selected by kind."
)]
pub enum AgentSource {
    #[schemars(title = "Certificate transparency")]
    CertificateTransparency(CertificateTransparency),
    #[schemars(title = "DNS CNAME")]
    DnsCname(DnsCname),
    #[schemars(title = "HTTP probe")]
    HttpProbe(HttpProbe),
    #[schemars(title = "BOAMP")]
    Boamp(KeywordSource),
    #[schemars(title = "DECP")]
    Decp(KeywordSource),
    #[schemars(title = "TED")]
    Ted(KeywordSource),
    #[schemars(title = "France Travail")]
    FranceTravail(KeywordSource),
    #[schemars(title = "ATS")]
    Ats(KeywordSource),
}

impl AgentSource {
    pub fn keyword_rules(&self) -> Option<&KeywordSource> {
        match self {
            Self::Boamp(rules)
            | Self::Decp(rules)
            | Self::Ted(rules)
            | Self::FranceTravail(rules)
            | Self::Ats(rules) => Some(rules),
            _ => None,
        }
    }

    pub fn certificate_transparency(&self) -> Option<&CertificateTransparency> {
        match self {
            Self::CertificateTransparency(rules) => Some(rules),
            _ => None,
        }
    }

    pub fn dns_cname(&self) -> Option<&DnsCname> {
        match self {
            Self::DnsCname(rules) => Some(rules),
            _ => None,
        }
    }

    pub fn http_probe(&self) -> Option<&HttpProbe> {
        match self {
            Self::HttpProbe(rules) => Some(rules),
            _ => None,
        }
    }

    pub fn source_name(&self) -> SourceName {
        match self {
            Self::CertificateTransparency(_) => SourceName::CertificateTransparency,
            Self::DnsCname(_) => SourceName::DnsCname,
            Self::HttpProbe(_) => SourceName::HttpProbe,
            Self::Boamp(_) => SourceName::Boamp,
            Self::Decp(_) => SourceName::Decp,
            Self::Ted(_) => SourceName::Ted,
            Self::FranceTravail(_) => SourceName::FranceTravail,
            Self::Ats(_) => SourceName::Ats,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Agent LLM settings",
    description = "LLM settings of one agent."
)]
pub struct AgentLlm {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(
        description = "Criteria given to the LLM to triage this agent's candidates. The profile criteria apply when absent."
    )]
    pub relevance_criteria: Option<String>,
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
    #[schemars(
        range(min = 1, max = 10),
        description = "Integer weight from 1 to 10 applied to the measure of the signal."
    )]
    pub weight: u8,
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
    description = "Qualification threshold, relevance criteria and answer language given to the model."
)]
pub struct LlmSettings {
    #[schemars(
        range(min = 0, max = 100),
        description = "Percentage of the maximum possible score from which a lead is qualified."
    )]
    pub qualification_threshold: u8,
    #[schemars(description = "Criteria given to the LLM to judge a lead relevance.")]
    pub relevance_criteria: String,
    #[serde(default)]
    #[schemars(
        title = "Answer language",
        description = "Language of every free-text field the LLM writes: triage reason, qualification summary, approach angle and draft. Evidence quotes stay in the source language."
    )]
    pub language: Language,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Fr,
    En,
}

impl Language {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fr => "French",
            Self::En => "English",
        }
    }
}

impl ProfileDefinition {
    pub fn max_score(&self) -> f64 {
        crate::domain::scoring::max_score(&self.scoring)
    }

    pub fn qualification_points(&self) -> f64 {
        f64::from(self.llm.qualification_threshold) * self.max_score() / 100.0
    }

    pub fn enabled_agents(&self, source: SourceName) -> impl Iterator<Item = &Agent> {
        self.agents
            .iter()
            .filter(move |agent| agent.enabled && agent.source.source_name() == source)
    }

    pub fn enabled_keyword_rules(
        &self,
        source: SourceName,
    ) -> impl Iterator<Item = &KeywordSource> {
        self.enabled_agents(source)
            .filter_map(|agent| agent.source.keyword_rules())
    }
}
