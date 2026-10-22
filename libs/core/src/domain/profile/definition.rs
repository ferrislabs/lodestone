use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::values::SourceName;

pub const PROFILE_SCHEMA_VERSION: u32 = 19;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Detection profile",
    description = "Describes one market: what is sold, which agents scan which sources, which qualification agents judge a lead, which scoring agents decide its score and verdict, how an aggregator sums it up and which contacts to prefer."
)]
pub struct ProfileDefinition {
    #[schemars(description = "Profile format version. Must be 19.")]
    pub schema_version: u32,
    pub identity: Identity,
    #[serde(default)]
    #[schemars(
        title = "Answer language",
        description = "Language of every free-text field the LLM writes: triage reason, qualification summary, approach angle and draft. Evidence quotes stay in the source language."
    )]
    pub language: Language,
    #[schemars(
        description = "Sourcing agents: they find leads. At least one is required unless the profile probes its own fingerprints or highlights job keywords."
    )]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub inspection: Inspection,
    #[serde(default)]
    pub enrichment: Enrichment,
    #[schemars(
        description = "Qualification agents. Each one evaluates every lead of the profile from one angle. At least one is required."
    )]
    pub qualification_agents: Vec<QualificationAgent>,
    #[serde(default)]
    #[schemars(
        description = "Scoring agents. Each one decides a score and a verdict for every lead from the reports of the qualification agents, through a contract imposed by the application. The lead score and verdict are the means of their answers weighted by the weight of each agent. With none enabled, the verdict stays pending."
    )]
    pub scoring_agents: Vec<ScoringAgent>,
    pub aggregator: Aggregator,
    #[serde(default)]
    pub escalation: Escalation,
    #[serde(default)]
    pub verdict: VerdictThresholds,
    #[serde(default)]
    pub evidence: EvidenceSettings,
    pub contact_ranking: ContactRanking,
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
    #[schemars(
        length(min = 1, max = 300),
        description = "What this agent evaluates, in one sentence, for example the technical maturity of the lead."
    )]
    pub description: String,
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
    #[serde(default = "default_exclusion_threshold")]
    #[schemars(
        range(min = 0.5, max = 1.0),
        description = "Probability from 0.5 to 1 at which a grounded answer for an option that excludes the lead excludes it. Default 0.6."
    )]
    pub exclusion_threshold: f64,
    #[serde(default = "default_samples")]
    #[schemars(
        range(min = 1, max = 5),
        description = "Number of independent model calls per lead, from 1 to 5. The answer of each question is the majority choice. Default 3."
    )]
    pub samples: u8,
    #[serde(default = "default_sampling_temperature")]
    #[schemars(
        range(min = 0.0, max = 1.5),
        description = "Temperature of the calls when samples is above 1, overriding the provider temperature. Default 0.7."
    )]
    pub sampling_temperature: f64,
    #[schemars(
        length(min = 1),
        description = "Questions the agent answers. At least one is required."
    )]
    pub questions: Vec<Question>,
    #[serde(default = "default_enabled")]
    #[schemars(
        description = "When true, once at least 50 leads are labelled with 10 of each class, the score of this agent is corrected by an isotonic map learned daily from the human decisions. The fit always runs so the quality page keeps its figures. Default true."
    )]
    pub learned_calibration: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Scoring agent",
    description = "Decides the score and the verdict of every lead from the reports of the qualification agents. The shape of its answer is imposed by the application: criteria choices, a score from 0 to 100, probabilities for each verdict and reasons."
)]
pub struct ScoringAgent {
    #[schemars(
        description = "Unique slug among all agents of the profile, sourcing, qualification and scoring, for example score-global."
    )]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[schemars(
        length(min = 1, max = 300),
        description = "What this agent evaluates, in one sentence."
    )]
    pub description: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[serde(default)]
    #[schemars(
        with = "Option<String>",
        description = "Id of a declared LLM provider. The default provider of the aggregator tier applies when null."
    )]
    pub provider_id: Option<Uuid>,
    #[serde(default = "default_weight")]
    #[schemars(
        range(min = 1, max = 10),
        description = "Integer weight from 1 to 10 of this agent in the lead score and verdict. It only matters when the profile has two or more scoring agents."
    )]
    pub weight: u8,
    #[schemars(
        length(min = 1, max = 8000),
        description = "System prompt given to the model."
    )]
    pub system_prompt: String,
    #[schemars(
        length(min = 1),
        description = "Criteria the agent answers before it scores. Same rules as qualification questions, except that no option can exclude a lead. At least one is required."
    )]
    pub criteria: Vec<Question>,
    #[serde(default = "default_scoring_samples")]
    #[schemars(
        range(min = 1, max = 5),
        description = "Number of independent model calls per lead, from 1 to 5. The criteria are voted, the score and the verdict probabilities are averaged. Default 1."
    )]
    pub samples: u8,
    #[serde(default = "default_sampling_temperature")]
    #[schemars(
        range(min = 0.0, max = 1.5),
        description = "Temperature of the calls when samples is above 1, overriding the provider temperature. Default 0.7."
    )]
    pub sampling_temperature: f64,
    #[serde(default = "default_enabled")]
    #[schemars(
        description = "When true, once at least 50 leads are labelled with 10 of each class, the verdict probabilities of this agent are corrected by an isotonic map learned daily from the human decisions. The fit always runs so the quality page keeps its figures. Default true."
    )]
    pub learned_calibration: bool,
}

pub const DEFAULT_SCORING_SAMPLES: u8 = 1;

pub fn default_scoring_samples() -> u8 {
    DEFAULT_SCORING_SAMPLES
}

pub const MAX_PROMPT_CHARS: usize = 8000;
pub const MAX_DESCRIPTION_CHARS: usize = 300;
pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 11;
pub const UNKNOWN_VALUE: &str = "inconnu";
pub const UNKNOWN_LABEL: &str = "Inconnu";
pub const UNKNOWN_SCORE: f64 = 0.5;
pub const EVIDENCE_MAX_CHARS: usize = 300;

pub const DEFAULT_EXCLUSION_THRESHOLD: f64 = 0.6;
pub const MIN_EXCLUSION_THRESHOLD: f64 = 0.5;

fn default_weight() -> u8 {
    5
}

pub fn default_exclusion_threshold() -> f64 {
    DEFAULT_EXCLUSION_THRESHOLD
}

pub const DEFAULT_SAMPLES: u8 = 3;
pub const MAX_SAMPLES: u8 = 5;
pub const DEFAULT_SAMPLING_TEMPERATURE: f64 = 0.7;
pub const MAX_SAMPLING_TEMPERATURE: f64 = 1.5;
pub const DEFAULT_BAND_LOW: u8 = 40;
pub const DEFAULT_BAND_HIGH: u8 = 70;

pub fn default_samples() -> u8 {
    DEFAULT_SAMPLES
}

pub fn default_sampling_temperature() -> f64 {
    DEFAULT_SAMPLING_TEMPERATURE
}

fn default_band_low() -> u8 {
    DEFAULT_BAND_LOW
}

fn default_band_high() -> u8 {
    DEFAULT_BAND_HIGH
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
        length(min = 2, max = 11),
        description = "Possible answers, from 2 to 11, with unique values. Exactly one of them is the unknown option."
    )]
    pub options: Vec<QuestionOption>,
}

impl Question {
    pub fn unknown_option(&self) -> Option<&QuestionOption> {
        self.options.iter().find(|option| option.is_unknown)
    }
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
    #[serde(default)]
    #[schemars(
        description = "Marks the unknown option, chosen when nothing in the documents supports an answer. Exactly one option per question carries it."
    )]
    pub is_unknown: bool,
    #[serde(default)]
    #[schemars(
        description = "When true, a grounded answer for this option at or above the agent's exclusion threshold excludes the lead. Never set on the unknown option."
    )]
    pub excludes: bool,
}

impl QuestionOption {
    pub fn unknown() -> Self {
        Self {
            value: UNKNOWN_VALUE.to_owned(),
            label: UNKNOWN_LABEL.to_owned(),
            score: UNKNOWN_SCORE,
            is_unknown: true,
            excludes: false,
        }
    }
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
    title = "Escalation",
    description = "Re-evaluates the leads whose confidence falls in a grey band with a stronger provider."
)]
pub struct Escalation {
    #[serde(default)]
    #[schemars(
        with = "Option<String>",
        description = "Id of a declared LLM provider that re-evaluates the grey-zone leads. Escalation is disabled when null."
    )]
    pub provider: Option<Uuid>,
    #[serde(default = "default_band_low")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is escalated from this confidence percentage. Must be lower than band_high. Default 40."
    )]
    pub band_low: u8,
    #[serde(default = "default_band_high")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is escalated below this confidence percentage. Default 70."
    )]
    pub band_high: u8,
}

impl Default for Escalation {
    fn default() -> Self {
        Self {
            provider: None,
            band_low: default_band_low(),
            band_high: default_band_high(),
        }
    }
}

impl Escalation {
    pub fn triggers(&self, confidence: f64) -> bool {
        self.provider.is_some()
            && confidence >= f64::from(self.band_low)
            && confidence < f64::from(self.band_high)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Verdict thresholds",
    description = "Rules that decide the verdict of a lead from its confidence: reliable, to check or discarded."
)]
pub struct VerdictThresholds {
    #[serde(default = "default_reliable_from")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is reliable from this confidence percentage, provided it also has two independent angles and enough evidence."
    )]
    pub reliable_from: u8,
    #[serde(default = "default_discard_below")]
    #[schemars(
        range(min = 0, max = 100),
        description = "A lead is discarded below this confidence percentage. Must be lower than reliable_from."
    )]
    pub discard_below: u8,
    #[serde(default = "default_calibration_floor")]
    #[schemars(
        range(min = 0.0, max = 0.2),
        description = "Each answer probability is clamped to [floor, 1 - floor] then renormalised before scoring, so a small model cannot claim absolute certainty. 0 disables the calibration."
    )]
    pub calibration_floor: f64,
    #[serde(default = "default_min_evidence")]
    #[schemars(
        range(min = 0.0, max = 1.0),
        description = "A lead is reliable only when its evidence strength (0 to 1) reaches this value."
    )]
    pub min_evidence: f64,
}

pub const MAX_CALIBRATION_FLOOR: f64 = 0.2;

fn default_reliable_from() -> u8 {
    70
}

fn default_discard_below() -> u8 {
    30
}

fn default_calibration_floor() -> f64 {
    0.05
}

fn default_min_evidence() -> f64 {
    0.5
}

impl Default for VerdictThresholds {
    fn default() -> Self {
        Self {
            reliable_from: default_reliable_from(),
            discard_below: default_discard_below(),
            calibration_floor: default_calibration_floor(),
            min_evidence: default_min_evidence(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Evidence weights",
    description = "Relative weight of each component of the evidence strength. A weight of 0 ignores the component."
)]
pub struct EvidenceWeights {
    #[serde(default = "default_evidence_weight")]
    #[schemars(
        range(min = 0, max = 10),
        description = "Number of independent signals, each decayed by its age."
    )]
    pub volume: u8,
    #[serde(default = "default_evidence_weight")]
    #[schemars(
        range(min = 0, max = 10),
        description = "Number of distinct sources and signal kinds."
    )]
    pub diversity: u8,
    #[serde(default = "default_evidence_weight")]
    #[schemars(
        range(min = 0, max = 10),
        description = "Share of production hosts among the hosts carrying signals."
    )]
    pub environment: u8,
    #[serde(default = "default_evidence_weight")]
    #[schemars(
        range(min = 0, max = 10),
        description = "The strongest independent signal raised by the next two, each strength given by its source and decayed by its age, so a weaker signal never lowers it."
    )]
    pub strength: u8,
}

fn default_evidence_weight() -> u8 {
    1
}

impl Default for EvidenceWeights {
    fn default() -> Self {
        Self {
            volume: default_evidence_weight(),
            diversity: default_evidence_weight(),
            environment: default_evidence_weight(),
            strength: default_evidence_weight(),
        }
    }
}

pub const DEFAULT_HALF_LIFE_KEY: &str = "default";
pub const FALLBACK_HALF_LIFE_DAYS: u32 = 180;
pub const MIN_HALF_LIFE_DAYS: u32 = 1;
pub const MAX_HALF_LIFE_DAYS: u32 = 3650;

const BUILT_IN_HALF_LIVES: [(&str, u32); 9] = [
    ("job_offer", 90),
    ("public_tender", 180),
    ("awarded_contract", 365),
    ("saas_provider", 365),
    ("product_detected", 365),
    ("debt_marker", 365),
    ("contact", 365),
    ("role_contact_found", 365),
    (DEFAULT_HALF_LIFE_KEY, FALLBACK_HALF_LIFE_DAYS),
];

pub(super) fn default_half_life_days() -> BTreeMap<String, u32> {
    BUILT_IN_HALF_LIVES
        .iter()
        .map(|(kind, days)| ((*kind).to_owned(), *days))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Evidence settings",
    description = "How the strength of the evidence behind a lead (signals, sources, hosts) weighs on its confidence."
)]
pub struct EvidenceSettings {
    #[serde(default = "default_influence")]
    #[schemars(
        range(min = 0.0, max = 1.0),
        description = "Confidence = agents opinion x ((1 - influence) + influence x evidence strength). 0 ignores the evidence, 1 multiplies the opinion by it."
    )]
    pub influence: f64,
    #[serde(default)]
    pub weights: EvidenceWeights,
    #[serde(default = "default_half_life_days")]
    #[schemars(
        description = "Half-life in days of a signal, by signal kind. A signal of that age counts for half in the volume and the strength of the evidence. The `default` key applies to every kind not listed; a kind that is not listed falls back to its built-in value.",
        extend("additionalProperties" = {"type": "integer", "minimum": 1, "maximum": 3650})
    )]
    pub half_life_days: BTreeMap<String, u32>,
}

impl EvidenceSettings {
    pub fn half_life_days_for(&self, kind: &str) -> u32 {
        self.half_life_days
            .get(kind)
            .copied()
            .or_else(|| {
                BUILT_IN_HALF_LIVES
                    .iter()
                    .find(|(built_in, _)| *built_in == kind)
                    .map(|(_, days)| *days)
            })
            .or_else(|| self.half_life_days.get(DEFAULT_HALF_LIFE_KEY).copied())
            .unwrap_or(FALLBACK_HALF_LIFE_DAYS)
    }
}

pub(super) fn default_influence() -> f64 {
    0.6
}

impl Default for EvidenceSettings {
    fn default() -> Self {
        Self {
            influence: default_influence(),
            weights: EvidenceWeights::default(),
            half_life_days: default_half_life_days(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
#[schemars(
    title = "Agent",
    description = "A sourcing agent, selected by type. A search agent looks for keywords across catalog providers, a certificates agent reads hostnames in certificate transparency logs, a DNS agent recognises providers by the CNAME target of hosts."
)]
pub enum Agent {
    #[schemars(title = "Search agent")]
    Search(SearchAgent),
    #[schemars(title = "Certificates agent")]
    Certificates(CertificatesAgent),
    #[schemars(title = "DNS agent")]
    Dns(DnsAgent),
}

impl Agent {
    pub fn id(&self) -> &AgentId {
        match self {
            Self::Search(agent) => &agent.id,
            Self::Certificates(agent) => &agent.id,
            Self::Dns(agent) => &agent.id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Search(agent) => &agent.name,
            Self::Certificates(agent) => &agent.name,
            Self::Dns(agent) => &agent.name,
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Self::Search(agent) => agent.enabled,
            Self::Certificates(agent) => agent.enabled,
            Self::Dns(agent) => agent.enabled,
        }
    }

    pub fn as_search(&self) -> Option<&SearchAgent> {
        match self {
            Self::Search(agent) => Some(agent),
            _ => None,
        }
    }

    pub fn certificates(&self) -> Option<&CertificatesAgent> {
        match self {
            Self::Certificates(agent) => Some(agent),
            _ => None,
        }
    }

    pub fn dns(&self) -> Option<&DnsAgent> {
        match self {
            Self::Dns(agent) => Some(agent),
            _ => None,
        }
    }

    pub fn host_countries(&self) -> Option<&[CountryCode]> {
        match self {
            Self::Search(_) => None,
            Self::Certificates(agent) => Some(&agent.countries),
            Self::Dns(agent) => Some(&agent.countries),
        }
    }

    pub fn serves(&self, source: SourceName) -> bool {
        match self {
            Self::Search(agent) => agent.searches(source.as_str()),
            Self::Certificates(_) => source == SourceName::CertificateTransparency,
            Self::Dns(_) => source == SourceName::DnsCname,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Certificates agent",
    description = "Reads new certificates in the certificate transparency logs. A hostname with one of the labels creates or updates a lead for its registrable domain."
)]
pub struct CertificatesAgent {
    #[schemars(description = "Unique slug in the profile, for example certificats.")]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[schemars(description = "Hostname labels to look for, for example auth or sso.")]
    pub labels: Vec<String>,
    #[serde(default)]
    #[schemars(
        description = "Target countries, as ISO 3166-1 alpha-2 codes, uppercase, for example FR. A host under the country-code top-level domain of another country creates no lead; a host under a generic top-level domain creates one, suppressed for this profile once the head office of the company is known to be elsewhere. Empty means every country."
    )]
    pub countries: Vec<CountryCode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "DNS agent",
    description = "Resolves the CNAME chain of candidate hosts. A target matching a pattern creates or updates a lead for the registrable domain of the host."
)]
pub struct DnsAgent {
    #[schemars(description = "Unique slug in the profile, for example dns.")]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[schemars(description = "Provider and target suffix pairs.")]
    pub patterns: Vec<CnamePattern>,
    #[serde(default)]
    #[schemars(
        description = "Target countries, as ISO 3166-1 alpha-2 codes, uppercase, for example FR. A host under the country-code top-level domain of another country creates no lead; a host under a generic top-level domain creates one, suppressed for this profile once the head office of the company is known to be elsewhere. Empty means every country."
    )]
    pub countries: Vec<CountryCode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Ecosystem inspection",
    description = "Every lead with a registrable domain gets the same ecosystem inspection, whatever the profile: official identity, legal life, public contracts, site and technical stack, DNS and e-mail, hiring and recent activity. The qualification agents read what it finds and decide what matters."
)]
pub struct Inspection {
    #[serde(default = "default_enabled")]
    #[schemars(
        description = "When false, the leads of the profile are not inspected and qualification does not wait for the inspection. Default true."
    )]
    pub enabled: bool,
}

impl Default for Inspection {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Enrichment",
    description = "Settings of the inspection specific to this profile. Everything else is the same for every profile."
)]
pub struct Enrichment {
    #[serde(default)]
    pub custom: CustomInspection,
    #[serde(default)]
    #[schemars(
        description = "Keywords to highlight in the job offers of a lead. An offer with them becomes a job_offer signal. Empty means the keywords of the enabled search agents."
    )]
    pub highlight_keywords: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Profile-specific fingerprints",
    description = "Fingerprints probed on the site of every lead of this profile, with the products to ignore and the extra subdomains to probe. A match becomes a product_detected or debt_marker signal."
)]
pub struct CustomInspection {
    #[serde(default)]
    #[schemars(description = "Fingerprints to probe on each host of a lead.")]
    pub fingerprints: Vec<Fingerprint>,
    #[serde(default)]
    #[schemars(description = "Products to ignore, for example your own.")]
    pub excluded_products: Vec<String>,
    #[serde(default)]
    #[schemars(
        description = "Subdomains of the lead domain probed as well, as hostname labels. Empty by default."
    )]
    pub extra_subdomains: Vec<String>,
}

pub const COMPANY_JOBS_AGENT_ID: &str = "company-jobs";
pub const COMPANY_JOBS_NAME: &str = "Offres d'emploi de l'entreprise";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SourceTarget<'a> {
    Agent(&'a Agent),
    HttpProbe(&'a CustomInspection),
    CompanyJobs(&'a Enrichment),
}

impl<'a> SourceTarget<'a> {
    pub fn agent_id(&self) -> Option<&'a AgentId> {
        match self {
            Self::Agent(agent) => Some(agent.id()),
            Self::HttpProbe(_) | Self::CompanyJobs(_) => None,
        }
    }

    pub fn certificates(&self) -> Option<&'a CertificatesAgent> {
        match self {
            Self::Agent(agent) => agent.certificates(),
            _ => None,
        }
    }

    pub fn dns(&self) -> Option<&'a DnsAgent> {
        match self {
            Self::Agent(agent) => agent.dns(),
            _ => None,
        }
    }

    pub fn host_countries(&self) -> Option<&'a [CountryCode]> {
        match self {
            Self::Agent(agent) => agent.host_countries(),
            _ => None,
        }
    }

    pub fn http_probe(&self) -> Option<&'a CustomInspection> {
        match self {
            Self::HttpProbe(probe) => Some(probe),
            _ => None,
        }
    }
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Search agent",
    description = "Looks for keywords across one or several catalog providers. Keywords, exclusions and filters are defined once and every provider translates them into its own API, or applies them after collection when it cannot filter natively."
)]
pub struct SearchAgent {
    #[schemars(description = "Unique slug in the profile, for example marches-publics.")]
    pub id: AgentId,
    #[schemars(description = "Display name.")]
    pub name: String,
    #[serde(default = "default_enabled")]
    #[schemars(description = "A disabled agent is kept but not run.")]
    pub enabled: bool,
    #[schemars(
        length(min = 1),
        description = "Catalog provider keys searched by this agent, for example boamp, decp, ted, france_travail or hacker_news_hiring. Each one must exist in the catalog and support keyword search. A provider that is not ready is skipped at run time."
    )]
    pub providers: Vec<String>,
    #[schemars(description = "Keywords to search for.")]
    pub keywords: Vec<String>,
    #[serde(default)]
    #[schemars(
        description = "Keywords that reject a candidate when present, even if it matches enough keywords."
    )]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub filters: SearchFilters,
    #[serde(default = "default_accept_min_keywords")]
    #[schemars(
        description = "Distinct keywords a candidate needs for a strong signal. Fewer still keep the candidate with a weaker signal."
    )]
    pub accept_min_keywords: u8,
}

pub(super) fn default_accept_min_keywords() -> u8 {
    2
}

impl SearchAgent {
    pub fn new(
        id: AgentId,
        name: impl Into<String>,
        providers: &[&str],
        keywords: &[&str],
        exclusions: &[&str],
    ) -> Self {
        Self {
            id,
            name: name.into(),
            enabled: true,
            providers: providers.iter().map(|key| (*key).to_owned()).collect(),
            keywords: keywords.iter().map(|word| (*word).to_owned()).collect(),
            exclusions: exclusions.iter().map(|word| (*word).to_owned()).collect(),
            filters: SearchFilters::default(),
            accept_min_keywords: default_accept_min_keywords(),
        }
    }

    pub fn searches(&self, provider: &str) -> bool {
        self.providers.iter().any(|key| key == provider)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    title = "Search filters",
    description = "Criteria shared by every provider of a search agent. A provider filters natively what its API supports and the rest is applied on the candidate text after collection."
)]
pub struct SearchFilters {
    #[serde(default)]
    #[schemars(description = "Remote work requirement. Default any.")]
    pub remote: RemoteFilter,
    #[serde(default)]
    #[schemars(
        description = "ISO 3166-1 alpha-2 country codes, uppercase, for example FR. Empty means every country."
    )]
    pub countries: Vec<CountryCode>,
    #[serde(default)]
    #[schemars(description = "Accepted contract types. Empty, or any, means every contract type.")]
    pub contracts: Vec<ContractFilter>,
}

impl SearchFilters {
    pub fn is_open(&self) -> bool {
        self.remote == RemoteFilter::Any
            && self.countries.is_empty()
            && self
                .contracts
                .iter()
                .all(|kind| *kind == ContractFilter::Any)
    }

    pub fn wants_any_contract(&self) -> bool {
        self.contracts.is_empty() || self.contracts.contains(&ContractFilter::Any)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(
    title = "Remote filter",
    description = "any keeps every candidate, full_remote keeps fully remote ones, remote_or_hybrid keeps remote and hybrid ones."
)]
pub enum RemoteFilter {
    #[default]
    Any,
    FullRemote,
    RemoteOrHybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(title = "Contract filter")]
pub enum ContractFilter {
    Freelance,
    Employee,
    PublicContract,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(
    title = "Country code",
    description = "ISO 3166-1 alpha-2 country code, uppercase."
)]
pub struct CountryCode(#[schemars(length(min = 2, max = 2), regex(pattern = "^[A-Z]{2}$"))] String);

impl CountryCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        self.0.len() == 2 && self.0.bytes().all(|byte| byte.is_ascii_uppercase())
    }
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
    #[serde(default)]
    #[schemars(
        description = "Target countries, as ISO 3166-1 alpha-2 codes, uppercase, for example FR. A host under the country-code top-level domain of another country creates no lead; a host under a generic top-level domain creates one, suppressed for this profile once the head office of the company is known to be elsewhere. Empty means every country."
    )]
    pub countries: Vec<CountryCode>,
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
    #[serde(default)]
    #[schemars(
        description = "Target countries, as ISO 3166-1 alpha-2 codes, uppercase, for example FR. A host under the country-code top-level domain of another country creates no lead; a host under a generic top-level domain creates one, suppressed for this profile once the head office of the company is known to be elsewhere. Empty means every country."
    )]
    pub countries: Vec<CountryCode>,
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
    pub fn enabled_scoring_agents(&self) -> impl Iterator<Item = &ScoringAgent> {
        self.scoring_agents.iter().filter(|agent| agent.enabled)
    }

    pub fn enabled_agents(&self, source: SourceName) -> impl Iterator<Item = &Agent> {
        self.agents
            .iter()
            .filter(move |agent| agent.enabled() && agent.serves(source))
    }

    pub fn enabled_search_agents(&self, source: SourceName) -> impl Iterator<Item = &SearchAgent> {
        self.agents
            .iter()
            .filter_map(Agent::as_search)
            .filter(move |agent| agent.enabled && agent.searches(source.as_str()))
    }

    pub fn targets(&self, source: SourceName) -> Vec<SourceTarget<'_>> {
        let mut targets: Vec<SourceTarget<'_>> = self
            .enabled_agents(source)
            .map(SourceTarget::Agent)
            .collect();
        if source == SourceName::HttpProbe && self.probes_hosts() {
            targets.push(SourceTarget::HttpProbe(&self.enrichment.custom));
        }
        if source == SourceName::Ats && self.reads_company_jobs() {
            targets.push(SourceTarget::CompanyJobs(&self.enrichment));
        }
        targets
    }

    pub fn has_targets(&self, source: SourceName) -> bool {
        !self.targets(source).is_empty()
    }

    pub fn probes_hosts(&self) -> bool {
        self.inspection.enabled && !self.enrichment.custom.fingerprints.is_empty()
    }

    pub fn reads_company_jobs(&self) -> bool {
        self.inspection.enabled && !self.company_jobs_keywords().is_empty()
    }

    pub fn company_jobs_keywords(&self) -> Vec<String> {
        let own = &self.enrichment.highlight_keywords;
        if !own.is_empty() {
            return own.clone();
        }
        let mut seen: Vec<String> = Vec::new();
        let mut keywords = Vec::new();
        for agent in self
            .agents
            .iter()
            .filter_map(Agent::as_search)
            .filter(|agent| agent.enabled)
        {
            for keyword in &agent.keywords {
                let lowered = keyword.trim().to_lowercase();
                if !lowered.is_empty() && !seen.contains(&lowered) {
                    seen.push(lowered);
                    keywords.push(keyword.trim().to_owned());
                }
            }
        }
        keywords
    }

    pub fn company_jobs_rules(&self) -> Option<SearchAgent> {
        self.reads_company_jobs().then(|| SearchAgent {
            id: AgentId::new(COMPANY_JOBS_AGENT_ID),
            name: COMPANY_JOBS_NAME.to_owned(),
            enabled: true,
            providers: vec![SourceName::Ats.as_str().to_owned()],
            keywords: self.company_jobs_keywords(),
            exclusions: Vec::new(),
            filters: SearchFilters::default(),
            accept_min_keywords: default_accept_min_keywords(),
        })
    }
}
