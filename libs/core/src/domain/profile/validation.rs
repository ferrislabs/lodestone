use std::{collections::HashSet, fmt};

use regex::RegexBuilder;
use serde::Serialize;
use thiserror::Error;

use crate::domain::values::SignalKind;

use super::{
    definition::{
        Agent, AgentSource, Aggregator, CertificateTransparency, CnamePattern, ContactRanking,
        DebtRule, DnsCname, Escalation, EvidenceSettings, Fingerprint, HttpProbe, Identity,
        KeywordSource, LlmSettings, MAX_CALIBRATION_FLOOR, MAX_DESCRIPTION_CHARS,
        MAX_HALF_LIFE_DAYS, MAX_OPTIONS, MAX_PROMPT_CHARS, MAX_SAMPLES, MAX_SAMPLING_TEMPERATURE,
        MIN_EXCLUSION_THRESHOLD, MIN_HALF_LIFE_DAYS, MIN_OPTIONS, Matcher, PROFILE_SCHEMA_VERSION,
        ProfileDefinition, QualificationAgent, Question, Scoring, ScoringAgent, VerdictThresholds,
        VersionExtractor,
    },
    legacy::{
        ProfileDefinitionV1, ProfileDefinitionV2, ProfileDefinitionV3, ProfileDefinitionV4,
        ProfileDefinitionV5, ProfileDefinitionV6, ProfileDefinitionV7, ProfileDefinitionV8,
        ProfileDefinitionV10, ProfileDefinitionV11,
    },
};

const REGEX_SIZE_LIMIT: usize = 1 << 20;
const MAX_PATTERN_LENGTH: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProfileProblem {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error(
        "unsupported schema version {found}, expected 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 or {PROFILE_SCHEMA_VERSION}"
    )]
    UnsupportedSchemaVersion { found: u32 },
    #[error("must not be empty")]
    Empty,
    #[error("duplicate value `{0}`")]
    Duplicate(String),
    #[error("at least one agent is required")]
    NoAgent,
    #[error("at least one qualification agent is required")]
    NoQualificationAgent,
    #[error("must be a slug of lowercase letters, digits and hyphens, at most 63 characters")]
    InvalidAgentId,
    #[error("must be lowercase letters, digits, hyphens and underscores, at most 63 characters")]
    InvalidIdentifier,
    #[error("must have from {MIN_OPTIONS} to {MAX_OPTIONS} options")]
    InvalidOptionCount,
    #[error("must have exactly one option with `is_unknown` set to true")]
    UnknownOptionCount,
    #[error("must be a number from 0 to 1")]
    InvalidScore,
    #[error("must be a number from 0.5 to 1")]
    InvalidExclusionThreshold,
    #[error("must be an integer from 1 to {MAX_SAMPLES}")]
    InvalidSamples,
    #[error("must be a number from 0 to {MAX_SAMPLING_TEMPERATURE}")]
    InvalidSamplingTemperature,
    #[error("must be higher than `band_low`")]
    BandOrder,
    #[error("the unknown option cannot exclude the lead")]
    UnknownCannotExclude,
    #[error("a scoring criterion cannot exclude the lead")]
    CriterionCannotExclude,
    #[error("must be at most {MAX_PROMPT_CHARS} characters")]
    TooLong,
    #[error("must be at most {MAX_DESCRIPTION_CHARS} characters")]
    DescriptionTooLong,
    #[error("must be higher than `discard_below`")]
    ThresholdOrder,
    #[error("invalid regular expression")]
    InvalidPattern,
    #[error("regular expression needs one capture group")]
    MissingCaptureGroup,
    #[error("must be an absolute path starting with `/`")]
    InvalidPath,
    #[error("HTTP status must be between 100 and 599")]
    InvalidHttpStatus,
    #[error("must be a lowercase hostname label")]
    InvalidLabel,
    #[error("must be a lowercase domain suffix with at least one dot")]
    InvalidSuffix,
    #[error("must be the local part of an email address, without `@`")]
    InvalidLocalPart,
    #[error("must also be listed in `preferred_local_parts`")]
    GuessNotRanked,
    #[error("must be an integer from 1 to 10")]
    InvalidWeight,
    #[error("must be a percentage from 0 to 100")]
    InvalidThreshold,
    #[error("must be a number from 0 to 1")]
    InvalidFraction,
    #[error("must be a number from 0 to {MAX_CALIBRATION_FLOOR}")]
    InvalidCalibrationFloor,
    #[error("must be an integer from 0 to 10")]
    InvalidEvidenceWeight,
    #[error("at least one weight must be above 0")]
    NoEvidenceWeight,
    #[error("must be a number of at least 1")]
    InvalidReferenceScore,
    #[error("must be an integer from {MIN_HALF_LIFE_DAYS} to {MAX_HALF_LIFE_DAYS} days")]
    InvalidHalfLife,
    #[error("must be a lowercase snake_case signal kind, or `default`")]
    InvalidSignalKind,
    #[error("must be a dotted numeric version")]
    InvalidVersion,
    #[error("must be at least 1")]
    MustBePositive,
    #[error("`version_below` needs a version extractor on the fingerprint")]
    VersionBelowWithoutExtractor,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct ProfileValidationError {
    pub path: String,
    pub problem: ProfileProblem,
}

impl fmt::Display for ProfileValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.problem)
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ValidatedProfile(ProfileDefinition);

impl ValidatedProfile {
    pub fn parse(value: serde_json::Value) -> Result<Self, ProfileValidationError> {
        let version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok());
        let definition = match version {
            Some(1) => decode::<ProfileDefinitionV1>(value)?.upgrade(),
            Some(2) => decode::<ProfileDefinitionV2>(value)?.upgrade(),
            Some(3) => decode::<ProfileDefinitionV3>(value)?.upgrade(),
            Some(4) => decode::<ProfileDefinitionV4>(value)?.upgrade(),
            Some(5) => decode::<ProfileDefinitionV5>(value)?.upgrade(),
            Some(6) => decode::<ProfileDefinitionV6>(value)?.upgrade(),
            Some(7) => decode::<ProfileDefinitionV7>(value)?.upgrade(),
            Some(8) => decode::<ProfileDefinitionV8>(value)?.upgrade(),
            Some(9 | 10) => decode::<ProfileDefinitionV10>(value)?.upgrade(),
            Some(11) => decode::<ProfileDefinitionV11>(value)?.upgrade(),
            Some(PROFILE_SCHEMA_VERSION) | None => decode::<ProfileDefinition>(value)?,
            Some(found) => {
                return Err(Path::root()
                    .field("schema_version")
                    .fail(ProfileProblem::UnsupportedSchemaVersion { found }));
            }
        };
        Self::try_from(definition)
    }

    pub fn definition(&self) -> &ProfileDefinition {
        &self.0
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.0).unwrap_or(serde_json::Value::Null)
    }
}

impl TryFrom<ProfileDefinition> for ValidatedProfile {
    type Error = ProfileValidationError;

    fn try_from(definition: ProfileDefinition) -> Result<Self, Self::Error> {
        validate(&definition)?;
        Ok(Self(definition))
    }
}

fn decode<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, ProfileValidationError> {
    serde_path_to_error::deserialize(value).map_err(|error| ProfileValidationError {
        path: path_or_root(error.path().to_string()),
        problem: ProfileProblem::Malformed(error.inner().to_string()),
    })
}

fn path_or_root(path: String) -> String {
    if path.is_empty() || path == "." {
        "$".to_owned()
    } else {
        path
    }
}

struct Path(String);

impl Path {
    fn root() -> Self {
        Self(String::new())
    }

    fn field(&self, name: &str) -> Self {
        if self.0.is_empty() {
            Self(name.to_owned())
        } else {
            Self(format!("{}.{name}", self.0))
        }
    }

    fn index(&self, index: usize) -> Self {
        Self(format!("{}[{index}]", self.0))
    }

    fn fail(&self, problem: ProfileProblem) -> ProfileValidationError {
        ProfileValidationError {
            path: path_or_root(self.0.clone()),
            problem,
        }
    }
}

type Check = Result<(), ProfileValidationError>;

fn validate(definition: &ProfileDefinition) -> Check {
    let root = Path::root();
    if definition.schema_version != PROFILE_SCHEMA_VERSION {
        return Err(root
            .field("schema_version")
            .fail(ProfileProblem::UnsupportedSchemaVersion {
                found: definition.schema_version,
            }));
    }
    validate_identity(&definition.identity, &root.field("identity"))?;
    validate_agents(&definition.agents, &root.field("agents"))?;
    validate_qualification_agents(
        &definition.qualification_agents,
        &definition.agents,
        &root.field("qualification_agents"),
    )?;
    validate_scoring_agents(
        &definition.scoring_agents,
        &definition.agents,
        &definition.qualification_agents,
        &root.field("scoring_agents"),
    )?;
    validate_aggregator(&definition.aggregator, &root.field("aggregator"))?;
    validate_escalation(&definition.escalation, &root.field("escalation"))?;
    validate_verdict(&definition.verdict, &root.field("verdict"))?;
    validate_evidence(&definition.evidence, &root.field("evidence"))?;
    validate_scoring(&definition.scoring, &root.field("scoring"))?;
    validate_contact_ranking(&definition.contact_ranking, &root.field("contact_ranking"))?;
    validate_llm(&definition.llm, &root.field("llm"))
}

fn non_empty(value: &str, path: &Path) -> Check {
    if value.trim().is_empty() {
        Err(path.fail(ProfileProblem::Empty))
    } else {
        Ok(())
    }
}

fn non_empty_list<T>(items: &[T], path: &Path) -> Check {
    if items.is_empty() {
        Err(path.fail(ProfileProblem::Empty))
    } else {
        Ok(())
    }
}

fn unique<'a>(values: impl Iterator<Item = (usize, &'a str)>, at: impl Fn(usize) -> Path) -> Check {
    let mut seen = HashSet::new();
    for (index, value) in values {
        if !seen.insert(value) {
            return Err(at(index).fail(ProfileProblem::Duplicate(value.to_owned())));
        }
    }
    Ok(())
}

fn compile(pattern: &str, path: &Path) -> Result<regex::Regex, ProfileValidationError> {
    non_empty(pattern, path)?;
    if pattern.len() > MAX_PATTERN_LENGTH {
        return Err(path.fail(ProfileProblem::InvalidPattern));
    }
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|_| path.fail(ProfileProblem::InvalidPattern))
}

fn validate_identity(identity: &Identity, path: &Path) -> Check {
    non_empty(&identity.offer, &path.field("offer"))?;
    non_empty(&identity.target_customer, &path.field("target_customer"))
}

fn validate_agents(agents: &[Agent], path: &Path) -> Check {
    non_empty_list(agents, path).map_err(|_| path.fail(ProfileProblem::NoAgent))?;
    for (index, agent) in agents.iter().enumerate() {
        let id = path.index(index).field("id");
        if !agent.id.is_slug() {
            return Err(id.fail(ProfileProblem::InvalidAgentId));
        }
    }
    unique(
        agents.iter().map(|agent| agent.id.as_str()).enumerate(),
        |index| path.index(index).field("id"),
    )?;
    for (index, agent) in agents.iter().enumerate() {
        validate_agent(agent, &path.index(index))?;
    }
    Ok(())
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=63).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'_')
}

fn bounded_prompt(prompt: &str, path: &Path) -> Check {
    non_empty(prompt, path)?;
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(path.fail(ProfileProblem::TooLong));
    }
    Ok(())
}

fn valid_weight(weight: u8, path: &Path) -> Check {
    if (1..=10).contains(&weight) {
        Ok(())
    } else {
        Err(path.fail(ProfileProblem::InvalidWeight))
    }
}

fn validate_qualification_agents(
    agents: &[QualificationAgent],
    sourcing: &[Agent],
    path: &Path,
) -> Check {
    non_empty_list(agents, path).map_err(|_| path.fail(ProfileProblem::NoQualificationAgent))?;
    for (index, agent) in agents.iter().enumerate() {
        if !agent.id.is_slug() {
            return Err(path
                .index(index)
                .field("id")
                .fail(ProfileProblem::InvalidAgentId));
        }
    }
    let mut seen: HashSet<&str> = sourcing.iter().map(|agent| agent.id.as_str()).collect();
    for (index, agent) in agents.iter().enumerate() {
        if !seen.insert(agent.id.as_str()) {
            return Err(path
                .index(index)
                .field("id")
                .fail(ProfileProblem::Duplicate(agent.id.as_str().to_owned())));
        }
    }
    for (index, agent) in agents.iter().enumerate() {
        validate_qualification_agent(agent, &path.index(index))?;
    }
    Ok(())
}

fn validate_qualification_agent(agent: &QualificationAgent, path: &Path) -> Check {
    non_empty(&agent.name, &path.field("name"))?;
    let description = path.field("description");
    non_empty(&agent.description, &description)?;
    if agent.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(description.fail(ProfileProblem::DescriptionTooLong));
    }
    bounded_prompt(&agent.system_prompt, &path.field("system_prompt"))?;
    valid_weight(agent.weight, &path.field("weight"))?;
    if !(MIN_EXCLUSION_THRESHOLD..=1.0).contains(&agent.exclusion_threshold) {
        return Err(path
            .field("exclusion_threshold")
            .fail(ProfileProblem::InvalidExclusionThreshold));
    }
    if !(1..=MAX_SAMPLES).contains(&agent.samples) {
        return Err(path.field("samples").fail(ProfileProblem::InvalidSamples));
    }
    if !(0.0..=MAX_SAMPLING_TEMPERATURE).contains(&agent.sampling_temperature) {
        return Err(path
            .field("sampling_temperature")
            .fail(ProfileProblem::InvalidSamplingTemperature));
    }
    let questions = path.field("questions");
    non_empty_list(&agent.questions, &questions)?;
    for (index, question) in agent.questions.iter().enumerate() {
        if !is_identifier(&question.id) {
            return Err(questions
                .index(index)
                .field("id")
                .fail(ProfileProblem::InvalidIdentifier));
        }
    }
    unique(
        agent
            .questions
            .iter()
            .map(|question| question.id.as_str())
            .enumerate(),
        |index| questions.index(index).field("id"),
    )?;
    for (index, question) in agent.questions.iter().enumerate() {
        validate_question(question, &questions.index(index))?;
    }
    Ok(())
}

fn validate_scoring_agents(
    agents: &[ScoringAgent],
    sourcing: &[Agent],
    qualification: &[QualificationAgent],
    path: &Path,
) -> Check {
    for (index, agent) in agents.iter().enumerate() {
        if !agent.id.is_slug() {
            return Err(path
                .index(index)
                .field("id")
                .fail(ProfileProblem::InvalidAgentId));
        }
    }
    let mut seen: HashSet<&str> = sourcing
        .iter()
        .map(|agent| agent.id.as_str())
        .chain(qualification.iter().map(|agent| agent.id.as_str()))
        .collect();
    for (index, agent) in agents.iter().enumerate() {
        if !seen.insert(agent.id.as_str()) {
            return Err(path
                .index(index)
                .field("id")
                .fail(ProfileProblem::Duplicate(agent.id.as_str().to_owned())));
        }
    }
    for (index, agent) in agents.iter().enumerate() {
        validate_scoring_agent(agent, &path.index(index))?;
    }
    Ok(())
}

fn validate_scoring_agent(agent: &ScoringAgent, path: &Path) -> Check {
    non_empty(&agent.name, &path.field("name"))?;
    let description = path.field("description");
    non_empty(&agent.description, &description)?;
    if agent.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(description.fail(ProfileProblem::DescriptionTooLong));
    }
    bounded_prompt(&agent.system_prompt, &path.field("system_prompt"))?;
    valid_weight(agent.weight, &path.field("weight"))?;
    if !(1..=MAX_SAMPLES).contains(&agent.samples) {
        return Err(path.field("samples").fail(ProfileProblem::InvalidSamples));
    }
    if !(0.0..=MAX_SAMPLING_TEMPERATURE).contains(&agent.sampling_temperature) {
        return Err(path
            .field("sampling_temperature")
            .fail(ProfileProblem::InvalidSamplingTemperature));
    }
    let criteria = path.field("criteria");
    non_empty_list(&agent.criteria, &criteria)?;
    for (index, criterion) in agent.criteria.iter().enumerate() {
        if !is_identifier(&criterion.id) {
            return Err(criteria
                .index(index)
                .field("id")
                .fail(ProfileProblem::InvalidIdentifier));
        }
    }
    unique(
        agent
            .criteria
            .iter()
            .map(|criterion| criterion.id.as_str())
            .enumerate(),
        |index| criteria.index(index).field("id"),
    )?;
    for (index, criterion) in agent.criteria.iter().enumerate() {
        let criterion_path = criteria.index(index);
        validate_question(criterion, &criterion_path)?;
        if let Some(position) = criterion.options.iter().position(|option| option.excludes) {
            return Err(criterion_path
                .field("options")
                .index(position)
                .field("excludes")
                .fail(ProfileProblem::CriterionCannotExclude));
        }
    }
    Ok(())
}

fn validate_question(question: &Question, path: &Path) -> Check {
    non_empty(&question.wording, &path.field("wording"))?;
    valid_weight(question.weight, &path.field("weight"))?;
    let options = path.field("options");
    if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&question.options.len()) {
        return Err(options.fail(ProfileProblem::InvalidOptionCount));
    }
    for (index, option) in question.options.iter().enumerate() {
        let option_path = options.index(index);
        if !is_identifier(&option.value) {
            return Err(option_path
                .field("value")
                .fail(ProfileProblem::InvalidIdentifier));
        }
        non_empty(&option.label, &option_path.field("label"))?;
        if option.excludes && option.is_unknown {
            return Err(option_path
                .field("excludes")
                .fail(ProfileProblem::UnknownCannotExclude));
        }
        if !(0.0..=1.0).contains(&option.score) {
            return Err(option_path
                .field("score")
                .fail(ProfileProblem::InvalidScore));
        }
    }
    unique(
        question
            .options
            .iter()
            .map(|option| option.value.as_str())
            .enumerate(),
        |index| options.index(index).field("value"),
    )?;
    if question
        .options
        .iter()
        .filter(|option| option.is_unknown)
        .count()
        != 1
    {
        return Err(options.fail(ProfileProblem::UnknownOptionCount));
    }
    Ok(())
}

fn validate_aggregator(aggregator: &Aggregator, path: &Path) -> Check {
    bounded_prompt(&aggregator.system_prompt, &path.field("system_prompt"))
}

fn validate_escalation(escalation: &Escalation, path: &Path) -> Check {
    for (name, value) in [
        ("band_low", escalation.band_low),
        ("band_high", escalation.band_high),
    ] {
        if value > 100 {
            return Err(path.field(name).fail(ProfileProblem::InvalidThreshold));
        }
    }
    if escalation.band_high <= escalation.band_low {
        return Err(path.field("band_high").fail(ProfileProblem::BandOrder));
    }
    Ok(())
}

fn validate_verdict(verdict: &VerdictThresholds, path: &Path) -> Check {
    for (name, value) in [
        ("reliable_from", verdict.reliable_from),
        ("discard_below", verdict.discard_below),
    ] {
        if value > 100 {
            return Err(path.field(name).fail(ProfileProblem::InvalidThreshold));
        }
    }
    if verdict.reliable_from <= verdict.discard_below {
        return Err(path
            .field("reliable_from")
            .fail(ProfileProblem::ThresholdOrder));
    }
    if !(0.0..=MAX_CALIBRATION_FLOOR).contains(&verdict.calibration_floor) {
        return Err(path
            .field("calibration_floor")
            .fail(ProfileProblem::InvalidCalibrationFloor));
    }
    if !(0.0..=1.0).contains(&verdict.min_evidence) {
        return Err(path
            .field("min_evidence")
            .fail(ProfileProblem::InvalidFraction));
    }
    Ok(())
}

fn validate_evidence(evidence: &EvidenceSettings, path: &Path) -> Check {
    if !(0.0..=1.0).contains(&evidence.influence) {
        return Err(path
            .field("influence")
            .fail(ProfileProblem::InvalidFraction));
    }
    if !evidence.reference_score.is_finite() || evidence.reference_score < 1.0 {
        return Err(path
            .field("reference_score")
            .fail(ProfileProblem::InvalidReferenceScore));
    }
    let weights = path.field("weights");
    let all = [
        ("volume", evidence.weights.volume),
        ("diversity", evidence.weights.diversity),
        ("environment", evidence.weights.environment),
        ("strength", evidence.weights.strength),
    ];
    for (name, weight) in all {
        if weight > 10 {
            return Err(weights
                .field(name)
                .fail(ProfileProblem::InvalidEvidenceWeight));
        }
    }
    if all.iter().all(|(_, weight)| *weight == 0) {
        return Err(weights.fail(ProfileProblem::NoEvidenceWeight));
    }
    let half_lives = path.field("half_life_days");
    for (kind, days) in &evidence.half_life_days {
        let entry = half_lives.field(kind);
        if SignalKind::parse(kind).is_err() {
            return Err(entry.fail(ProfileProblem::InvalidSignalKind));
        }
        if !(MIN_HALF_LIFE_DAYS..=MAX_HALF_LIFE_DAYS).contains(days) {
            return Err(entry.fail(ProfileProblem::InvalidHalfLife));
        }
    }
    Ok(())
}

fn validate_agent(agent: &Agent, path: &Path) -> Check {
    non_empty(&agent.name, &path.field("name"))?;
    let source = path.field("source");
    match &agent.source {
        AgentSource::CertificateTransparency(ct) => validate_ct(ct, &source)?,
        AgentSource::DnsCname(dns) => validate_dns(dns, &source)?,
        AgentSource::HttpProbe(probe) => validate_http_probe(probe, &source)?,
        AgentSource::Boamp(rules)
        | AgentSource::Decp(rules)
        | AgentSource::Ted(rules)
        | AgentSource::FranceTravail(rules)
        | AgentSource::Ats(rules) => validate_keyword_source(rules, &source)?,
    }
    if let Some(criteria) = agent
        .llm
        .as_ref()
        .and_then(|llm| llm.relevance_criteria.as_deref())
    {
        non_empty(criteria, &path.field("llm").field("relevance_criteria"))?;
    }
    Ok(())
}

fn is_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn validate_ct(ct: &CertificateTransparency, path: &Path) -> Check {
    let labels = path.field("labels");
    non_empty_list(&ct.labels, &labels)?;
    for (index, label) in ct.labels.iter().enumerate() {
        if !is_label(label) {
            return Err(labels.index(index).fail(ProfileProblem::InvalidLabel));
        }
    }
    Ok(())
}

fn validate_http_probe(probe: &HttpProbe, path: &Path) -> Check {
    let fingerprints = path.field("fingerprints");
    non_empty_list(&probe.fingerprints, &fingerprints)?;
    unique(
        probe
            .fingerprints
            .iter()
            .map(|fingerprint| fingerprint.id.as_str())
            .enumerate(),
        |index| fingerprints.index(index).field("id"),
    )?;
    for (index, fingerprint) in probe.fingerprints.iter().enumerate() {
        validate_fingerprint(fingerprint, &fingerprints.index(index))?;
    }
    let excluded = path.field("excluded_products");
    for (index, product) in probe.excluded_products.iter().enumerate() {
        non_empty(product, &excluded.index(index))?;
    }
    Ok(())
}

fn validate_fingerprint(fingerprint: &Fingerprint, path: &Path) -> Check {
    non_empty(&fingerprint.id, &path.field("id"))?;
    non_empty(&fingerprint.product, &path.field("product"))?;
    if !fingerprint.path.starts_with('/') || fingerprint.path.contains("://") {
        return Err(path.field("path").fail(ProfileProblem::InvalidPath));
    }
    let matchers = path.field("matchers");
    non_empty_list(&fingerprint.matchers, &matchers)?;
    for (index, matcher) in fingerprint.matchers.iter().enumerate() {
        validate_matcher(matcher, &matchers.index(index))?;
    }
    if let Some(version) = &fingerprint.version {
        validate_version_extractor(version, &path.field("version"))?;
    }
    let markers = path.field("debt_markers");
    unique(
        fingerprint
            .debt_markers
            .iter()
            .map(|marker| marker.id.as_str())
            .enumerate(),
        |index| markers.index(index).field("id"),
    )?;
    for (index, marker) in fingerprint.debt_markers.iter().enumerate() {
        let marker_path = markers.index(index);
        non_empty(&marker.id, &marker_path.field("id"))?;
        let rule = marker_path.field("rule");
        match &marker.rule {
            DebtRule::Matches { matcher } => validate_matcher(matcher, &rule.field("matcher"))?,
            DebtRule::VersionBelow { version } => {
                if fingerprint.version.is_none() {
                    return Err(rule.fail(ProfileProblem::VersionBelowWithoutExtractor));
                }
                if !is_dotted_version(version) {
                    return Err(rule.field("version").fail(ProfileProblem::InvalidVersion));
                }
            }
        }
    }
    Ok(())
}

fn validate_matcher(matcher: &Matcher, path: &Path) -> Check {
    match matcher {
        Matcher::Status { equals } => {
            if (100..=599).contains(equals) {
                Ok(())
            } else {
                Err(path.field("equals").fail(ProfileProblem::InvalidHttpStatus))
            }
        }
        Matcher::Header { name, pattern } => {
            non_empty(name, &path.field("name"))?;
            compile(pattern, &path.field("pattern")).map(drop)
        }
        Matcher::Body { pattern } => compile(pattern, &path.field("pattern")).map(drop),
    }
}

fn validate_version_extractor(extractor: &VersionExtractor, path: &Path) -> Check {
    let pattern = match extractor {
        VersionExtractor::Header { name, pattern } => {
            non_empty(name, &path.field("name"))?;
            pattern
        }
        VersionExtractor::Body { pattern } => pattern,
    };
    let pattern_path = path.field("pattern");
    let regex = compile(pattern, &pattern_path)?;
    if regex.captures_len() < 2 {
        return Err(pattern_path.fail(ProfileProblem::MissingCaptureGroup));
    }
    Ok(())
}

fn is_dotted_version(version: &str) -> bool {
    !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn validate_dns(dns: &DnsCname, path: &Path) -> Check {
    let patterns = path.field("patterns");
    non_empty_list(&dns.patterns, &patterns)?;
    for (index, pattern) in dns.patterns.iter().enumerate() {
        validate_cname_pattern(pattern, &patterns.index(index))?;
    }
    Ok(())
}

fn validate_cname_pattern(pattern: &CnamePattern, path: &Path) -> Check {
    non_empty(&pattern.provider, &path.field("provider"))?;
    let suffix = pattern.target_suffix.as_str();
    let valid = suffix.contains('.') && suffix.split('.').all(is_label);
    if valid {
        Ok(())
    } else {
        Err(path
            .field("target_suffix")
            .fail(ProfileProblem::InvalidSuffix))
    }
}

fn validate_keyword_source(source: &KeywordSource, path: &Path) -> Check {
    let keywords = path.field("keywords");
    non_empty_list(&source.keywords, &keywords)?;
    for (index, keyword) in source.keywords.iter().enumerate() {
        non_empty(keyword, &keywords.index(index))?;
    }
    if source.accept_min_keywords < 1 {
        return Err(path
            .field("accept_min_keywords")
            .fail(ProfileProblem::MustBePositive));
    }
    let exclusions = path.field("exclusions");
    for (index, exclusion) in source.exclusions.iter().enumerate() {
        non_empty(exclusion, &exclusions.index(index))?;
    }
    Ok(())
}

fn validate_scoring(scoring: &Scoring, path: &Path) -> Check {
    let factors = path.field("factors");
    non_empty_list(&scoring.factors, &factors)?;
    for (index, factor) in scoring.factors.iter().enumerate() {
        let factor_path = factors.index(index);
        non_empty(&factor.signal, &factor_path.field("signal"))?;
        if let Some(value) = &factor.value {
            non_empty(value, &factor_path.field("value"))?;
        }
        if !(1..=10).contains(&factor.weight) {
            return Err(factor_path
                .field("weight")
                .fail(ProfileProblem::InvalidWeight));
        }
    }
    Ok(())
}

fn validate_contact_ranking(ranking: &ContactRanking, path: &Path) -> Check {
    let parts = path.field("preferred_local_parts");
    for (index, part) in ranking.preferred_local_parts.iter().enumerate() {
        let part_path = parts.index(index);
        non_empty(part, &part_path)?;
        if part.contains('@') || part.contains(char::is_whitespace) {
            return Err(part_path.fail(ProfileProblem::InvalidLocalPart));
        }
    }
    unique(
        ranking
            .preferred_local_parts
            .iter()
            .map(String::as_str)
            .enumerate(),
        |index| parts.index(index),
    )?;
    let guesses = path.field("guess_local_parts");
    for (index, guess) in ranking.guess_local_parts.iter().enumerate() {
        if !ranking.preferred_local_parts.contains(guess) {
            return Err(guesses.index(index).fail(ProfileProblem::GuessNotRanked));
        }
    }
    unique(
        ranking
            .guess_local_parts
            .iter()
            .map(String::as_str)
            .enumerate(),
        |index| guesses.index(index),
    )
}

fn validate_llm(llm: &LlmSettings, path: &Path) -> Check {
    if llm.qualification_threshold > 100 {
        return Err(path
            .field("qualification_threshold")
            .fail(ProfileProblem::InvalidThreshold));
    }
    non_empty(&llm.relevance_criteria, &path.field("relevance_criteria"))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::domain::{profile::fixtures::MINIMAL_PROFILE, values::SourceName};

    fn minimal() -> Value {
        serde_json::from_str(MINIMAL_PROFILE).unwrap()
    }

    fn error_for(mutate: impl FnOnce(&mut Value)) -> ProfileValidationError {
        let mut value = minimal();
        mutate(&mut value);
        ValidatedProfile::parse(value).unwrap_err()
    }

    #[test]
    fn minimal_profile_is_valid_and_round_trips() {
        let profile = ValidatedProfile::parse(minimal()).unwrap();
        let reparsed = ValidatedProfile::parse(profile.to_json()).unwrap();
        assert_eq!(reparsed, profile);
    }

    #[test]
    fn unknown_field_is_reported_with_its_path() {
        let error = error_for(|v| v["identity"]["extra"] = json!(1));
        assert_eq!(error.path, "identity.extra");
        assert!(matches!(error.problem, ProfileProblem::Malformed(_)));
    }

    #[test]
    fn wrong_type_is_reported_with_its_path() {
        let error = error_for(|v| v["scoring"]["factors"][0]["weight"] = json!("high"));
        assert_eq!(error.path, "scoring.factors[0].weight");
    }

    #[test]
    fn weights_outside_one_to_ten_are_rejected() {
        for weight in [0, 11, 255] {
            let error = error_for(|v| v["scoring"]["factors"][0]["weight"] = json!(weight));
            assert_eq!(error.path, "scoring.factors[0].weight");
            assert_eq!(error.problem, ProfileProblem::InvalidWeight);
        }
        assert_eq!(
            ProfileProblem::InvalidWeight.to_string(),
            "must be an integer from 1 to 10"
        );
    }

    #[test]
    fn fractional_and_negative_weights_are_rejected_at_their_path() {
        for weight in [json!(2.5), json!(-1)] {
            let error = error_for(|v| v["scoring"]["factors"][0]["weight"] = weight);
            assert_eq!(error.path, "scoring.factors[0].weight");
        }
    }

    #[test]
    fn thresholds_above_one_hundred_are_rejected() {
        let error = error_for(|v| v["llm"]["qualification_threshold"] = json!(101));
        assert_eq!(error.path, "llm.qualification_threshold");
        assert_eq!(error.problem, ProfileProblem::InvalidThreshold);
        assert_eq!(
            ProfileProblem::InvalidThreshold.to_string(),
            "must be a percentage from 0 to 100"
        );
        let error = error_for(|v| v["llm"]["qualification_threshold"] = json!(-1));
        assert_eq!(error.path, "llm.qualification_threshold");
        let valid = error_for_ok(|v| v["llm"]["qualification_threshold"] = json!(100));
        assert_eq!(valid.definition().llm.qualification_threshold, 100);
    }

    fn error_for_ok(mutate: impl FnOnce(&mut Value)) -> ValidatedProfile {
        let mut value = minimal();
        mutate(&mut value);
        ValidatedProfile::parse(value).unwrap()
    }

    fn iam_scoring_v2() -> Value {
        json!({"factors": [
            {"signal": "product_detected", "value": "keycloak", "weight": 30},
            {"signal": "debt_marker", "value": "legacy_auth_path", "weight": 15},
            {"signal": "saas_provider", "weight": 10},
            {"signal": "job_offer", "weight": 25},
            {"signal": "public_tender", "weight": 20},
            {"signal": "role_contact_found", "weight": 5},
            {"signal": "distinct_signals", "weight": 5}
        ]})
    }

    fn weights(profile: &ValidatedProfile) -> Vec<u8> {
        profile
            .definition()
            .scoring
            .factors
            .iter()
            .map(|factor| factor.weight)
            .collect()
    }

    #[test]
    fn version_two_weights_and_threshold_are_rescaled() {
        let profile = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["scoring"] = iam_scoring_v2();
            v["llm"]["qualification_threshold"] = json!(50);
        });
        assert_eq!(profile.definition().schema_version, 12);
        assert_eq!(weights(&profile), [10, 5, 3, 8, 7, 2, 2]);
        assert_eq!(profile.definition().llm.qualification_threshold, 37);
        assert_eq!(profile.to_json()["schema_version"], 12);
    }

    #[test]
    fn version_two_weights_within_the_new_scale_are_kept() {
        let profile = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["scoring"] = json!({"factors": [
                {"signal": "a", "weight": 7.0},
                {"signal": "b", "weight": 2},
                {"signal": "distinct_signals", "weight": 1}
            ]});
            v["llm"]["qualification_threshold"] = json!(8.0);
        });
        assert_eq!(weights(&profile), [7, 2, 1]);
        assert_eq!(profile.definition().llm.qualification_threshold, 73);
    }

    #[test]
    fn version_two_fractional_weights_are_rescaled_and_floored_at_one() {
        let profile = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["scoring"] = json!({"factors": [
                {"signal": "a", "weight": 2.5},
                {"signal": "b", "weight": 100},
                {"signal": "c", "weight": 0}
            ]});
        });
        assert_eq!(weights(&profile), [1, 10, 1]);
    }

    #[test]
    fn version_two_threshold_is_clamped_and_falls_back_without_a_maximum() {
        let high = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["llm"]["qualification_threshold"] = json!(5000);
        });
        assert_eq!(high.definition().llm.qualification_threshold, 100);
        let negative = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["llm"]["qualification_threshold"] = json!(-4);
        });
        assert_eq!(negative.definition().llm.qualification_threshold, 0);
        let empty_max = error_for_ok(|v| {
            v["schema_version"] = json!(2);
            v["scoring"] = json!({"factors": [{"signal": "distinct_signals", "weight": 5}]});
        });
        assert_eq!(empty_max.definition().llm.qualification_threshold, 50);
    }

    #[test]
    fn version_one_documents_get_rescaled_scoring_too() {
        let profile = error_for_ok(|v| {
            v["schema_version"] = json!(1);
            v.as_object_mut().unwrap().remove("agents");
            v["sources"] = json!({"ats": {"keywords": ["sso"]}});
            v["scoring"] = iam_scoring_v2();
            v["llm"]["qualification_threshold"] = json!(50);
        });
        assert_eq!(weights(&profile), [10, 5, 3, 8, 7, 2, 2]);
        assert_eq!(profile.definition().llm.qualification_threshold, 37);
    }

    #[test]
    fn max_score_and_qualification_points_follow_the_profile() {
        let profile = error_for_ok(|v| {
            v["scoring"] = json!({"factors": [
                {"signal": "a", "weight": 10},
                {"signal": "b", "weight": 4},
                {"signal": "distinct_signals", "weight": 2}
            ]});
            v["llm"]["qualification_threshold"] = json!(50);
        });
        let definition = profile.definition();
        assert!((definition.max_score() - 18.0).abs() < 1e-9);
        assert!((definition.qualification_points() - 9.0).abs() < 1e-9);
    }

    #[test]
    fn missing_field_is_reported() {
        let error = error_for(|v| {
            v.as_object_mut().unwrap().remove("llm");
        });
        assert_eq!(error.path, "$");
        assert!(matches!(error.problem, ProfileProblem::Malformed(_)));
    }

    const PROBE: &str = "agents[0].source.fingerprints[0]";

    fn agent_json(id: &str, source: Value) -> Value {
        json!({"id": id, "name": id, "source": source})
    }

    fn push_agent(v: &mut Value, agent: Value) {
        v["agents"].as_array_mut().unwrap().push(agent);
    }

    #[test]
    fn unsupported_schema_version_is_rejected() {
        let error = error_for(|v| v["schema_version"] = json!(13));
        assert_eq!(error.path, "schema_version");
        assert_eq!(
            error.problem,
            ProfileProblem::UnsupportedSchemaVersion { found: 13 }
        );
        let error = error_for(|v| v["schema_version"] = json!(0));
        assert_eq!(
            error.problem,
            ProfileProblem::UnsupportedSchemaVersion { found: 0 }
        );
    }

    #[test]
    fn non_numeric_schema_version_is_malformed() {
        let error = error_for(|v| v["schema_version"] = json!("2"));
        assert_eq!(error.path, "schema_version");
        assert!(matches!(error.problem, ProfileProblem::Malformed(_)));
    }

    #[test]
    fn empty_offer_is_rejected() {
        let error = error_for(|v| v["identity"]["offer"] = json!("  "));
        assert_eq!(error.path, "identity.offer");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn profile_without_agents_is_rejected() {
        let error = error_for(|v| v["agents"] = json!([]));
        assert_eq!(error.path, "agents");
        assert_eq!(error.problem, ProfileProblem::NoAgent);
    }

    #[test]
    fn agent_id_must_be_a_slug() {
        for bad in ["", "Auth", "-a", "a b", "a_b", &"a".repeat(64)] {
            let error = error_for(|v| v["agents"][0]["id"] = json!(bad));
            assert_eq!(error.path, "agents[0].id", "{bad}");
            assert_eq!(error.problem, ProfileProblem::InvalidAgentId, "{bad}");
        }
        let long = "a".repeat(63);
        for good in ["a", "0", "ats-en", long.as_str()] {
            let mut value = minimal();
            value["agents"][0]["id"] = json!(good);
            assert!(ValidatedProfile::parse(value).is_ok(), "{good}");
        }
    }

    #[test]
    fn duplicate_agent_ids_are_rejected() {
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json("other", json!({"kind": "boamp", "keywords": ["sso"]})),
            );
            push_agent(
                v,
                agent_json("http-probe", json!({"kind": "ted", "keywords": ["sso"]})),
            );
        });
        assert_eq!(error.path, "agents[2].id");
        assert_eq!(
            error.problem,
            ProfileProblem::Duplicate("http-probe".to_owned())
        );
    }

    #[test]
    fn agent_name_must_not_be_empty() {
        let error = error_for(|v| v["agents"][0]["name"] = json!(" "));
        assert_eq!(error.path, "agents[0].name");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn agent_relevance_criteria_must_not_be_empty_when_present() {
        let error = error_for(|v| v["agents"][0]["llm"] = json!({"relevance_criteria": "  "}));
        assert_eq!(error.path, "agents[0].llm.relevance_criteria");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let mut value = minimal();
        value["agents"][0]["llm"] = json!({});
        assert!(ValidatedProfile::parse(value).is_ok());
    }

    #[test]
    fn agent_kind_is_required_and_known() {
        let error = error_for(|v| v["agents"][0]["source"]["kind"] = json!("rss"));
        assert_eq!(error.path, "agents[0].source.kind");
        assert!(matches!(error.problem, ProfileProblem::Malformed(_)));
    }

    #[test]
    fn agent_enabled_defaults_to_true() {
        let profile = ValidatedProfile::parse(minimal()).unwrap();
        assert!(profile.definition().agents[0].enabled);
    }

    #[test]
    fn invalid_regex_points_at_the_pattern() {
        let error = error_for(|v| {
            v["agents"][0]["source"]["fingerprints"][0]["matchers"][1]["pattern"] = json!("(")
        });
        assert_eq!(error.path, format!("{PROBE}.matchers[1].pattern"));
        assert_eq!(error.problem, ProfileProblem::InvalidPattern);
    }

    #[test]
    fn version_extractor_needs_a_capture_group() {
        let error = error_for(|v| {
            v["agents"][0]["source"]["fingerprints"][0]["version"]["pattern"] = json!("^\\d+")
        });
        assert_eq!(error.path, format!("{PROBE}.version.pattern"));
        assert_eq!(error.problem, ProfileProblem::MissingCaptureGroup);
    }

    #[test]
    fn version_below_needs_an_extractor() {
        let error = error_for(|v| {
            v["agents"][0]["source"]["fingerprints"][0]
                .as_object_mut()
                .unwrap()
                .remove("version");
        });
        assert_eq!(error.path, format!("{PROBE}.debt_markers[0].rule"));
        assert_eq!(error.problem, ProfileProblem::VersionBelowWithoutExtractor);
    }

    #[test]
    fn fingerprint_path_must_be_absolute() {
        let error = error_for(|v| {
            v["agents"][0]["source"]["fingerprints"][0]["path"] = json!("https://evil.test/x")
        });
        assert_eq!(error.path, format!("{PROBE}.path"));
        assert_eq!(error.problem, ProfileProblem::InvalidPath);
    }

    #[test]
    fn duplicate_fingerprint_ids_are_rejected() {
        let error = error_for(|v| {
            let first = v["agents"][0]["source"]["fingerprints"][0].clone();
            v["agents"][0]["source"]["fingerprints"]
                .as_array_mut()
                .unwrap()
                .push(first);
        });
        assert_eq!(error.path, "agents[0].source.fingerprints[1].id");
        assert!(matches!(error.problem, ProfileProblem::Duplicate(_)));
    }

    #[test]
    fn ct_labels_must_be_hostname_labels() {
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json(
                    "ct",
                    json!({"kind": "certificate_transparency", "labels": ["sso", "Auth Login"]}),
                ),
            )
        });
        assert_eq!(error.path, "agents[1].source.labels[1]");
        assert_eq!(error.problem, ProfileProblem::InvalidLabel);
    }

    #[test]
    fn cname_suffix_must_be_a_domain_suffix() {
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json(
                    "dns",
                    json!({"kind": "dns_cname", "patterns": [{"provider": "auth0", "target_suffix": "auth0"}]}),
                ),
            )
        });
        assert_eq!(error.path, "agents[1].source.patterns[0].target_suffix");
        assert_eq!(error.problem, ProfileProblem::InvalidSuffix);
    }

    #[test]
    fn keyword_source_needs_keywords() {
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json("boamp", json!({"kind": "boamp", "keywords": []})),
            )
        });
        assert_eq!(error.path, "agents[1].source.keywords");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn keyword_entries_must_not_be_empty() {
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json("ats", json!({"kind": "ats", "keywords": ["sso", " "]})),
            )
        });
        assert_eq!(error.path, "agents[1].source.keywords[1]");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn keyword_source_accept_threshold_defaults_and_must_be_positive() {
        let mut value = minimal();
        push_agent(
            &mut value,
            agent_json("boamp", json!({"kind": "boamp", "keywords": ["sso"]})),
        );
        let profile = ValidatedProfile::parse(value).unwrap();
        assert_eq!(
            profile
                .definition()
                .enabled_keyword_rules(SourceName::Boamp)
                .next()
                .unwrap()
                .accept_min_keywords,
            2
        );
        let error = error_for(|v| {
            push_agent(
                v,
                agent_json(
                    "boamp",
                    json!({"kind": "boamp", "keywords": ["sso"], "accept_min_keywords": 0}),
                ),
            )
        });
        assert_eq!(error.path, "agents[1].source.accept_min_keywords");
        assert_eq!(error.problem, ProfileProblem::MustBePositive);
    }

    #[test]
    fn source_fields_of_another_kind_are_rejected() {
        let error = error_for(|v| v["agents"][0]["source"]["keywords"] = json!(["sso"]));
        assert_eq!(error.path, "agents[0].source");
        assert!(matches!(error.problem, ProfileProblem::Malformed(_)));
    }

    fn two_ats_agents(first_enabled: bool) -> Value {
        let mut value = minimal();
        push_agent(
            &mut value,
            json!({"id": "ats-fr", "name": "Jobs FR", "enabled": first_enabled,
                "source": {"kind": "ats", "keywords": ["iam"]}}),
        );
        push_agent(
            &mut value,
            agent_json("ats-en", json!({"kind": "ats", "keywords": ["sso"]})),
        );
        value
    }

    fn ats_keywords(definition: &ProfileDefinition) -> Vec<&[String]> {
        definition
            .enabled_keyword_rules(SourceName::Ats)
            .map(|rules| rules.keywords.as_slice())
            .collect()
    }

    #[test]
    fn enabled_agents_lists_every_enabled_agent_of_the_kind() {
        let profile = ValidatedProfile::parse(two_ats_agents(true)).unwrap();
        let definition = profile.definition();
        assert_eq!(ats_keywords(definition), [["iam"], ["sso"]]);
        assert_eq!(definition.enabled_agents(SourceName::HttpProbe).count(), 1);
        assert_eq!(definition.enabled_agents(SourceName::Boamp).count(), 0);
    }

    #[test]
    fn enabled_agents_skip_disabled_agents() {
        let profile = ValidatedProfile::parse(two_ats_agents(false)).unwrap();
        assert_eq!(ats_keywords(profile.definition()), [["sso"]]);
    }

    #[test]
    fn disabled_only_kind_has_no_enabled_agent() {
        let mut value = minimal();
        value["agents"][0]["enabled"] = json!(false);
        let profile = ValidatedProfile::parse(value).unwrap();
        assert_eq!(
            profile
                .definition()
                .enabled_agents(SourceName::HttpProbe)
                .count(),
            0
        );
    }

    #[test]
    fn agent_criteria_are_trimmed_and_optional() {
        let profile = ValidatedProfile::parse(minimal()).unwrap();
        assert_eq!(profile.definition().agents[0].relevance_criteria(), None);
        let mut value = minimal();
        value["agents"][0]["llm"] = json!({"relevance_criteria": " tenders only "});
        let profile = ValidatedProfile::parse(value).unwrap();
        assert_eq!(
            profile.definition().agents[0].relevance_criteria(),
            Some("tenders only")
        );
    }

    #[test]
    fn source_name_follows_the_kind() {
        let profile = ValidatedProfile::parse(two_ats_agents(true)).unwrap();
        let names: Vec<_> = profile
            .definition()
            .agents
            .iter()
            .map(|agent| agent.source.source_name())
            .collect();
        assert_eq!(
            names,
            [SourceName::HttpProbe, SourceName::Ats, SourceName::Ats]
        );
    }

    #[test]
    fn version_one_documents_are_upgraded() {
        let mut value = minimal();
        let agent = value["agents"][0]["source"].clone();
        let mut source = agent.clone();
        source.as_object_mut().unwrap().remove("kind");
        value["schema_version"] = json!(1);
        value.as_object_mut().unwrap().remove("agents");
        value["sources"] = json!({"http_probe": source, "ats": {"keywords": ["sso"]}});
        let profile = ValidatedProfile::parse(value).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        let summary: Vec<_> = definition
            .agents
            .iter()
            .map(|agent| (agent.id.as_str(), agent.name.as_str(), agent.enabled))
            .collect();
        assert_eq!(
            summary,
            [("http-probe", "HTTP probe", true), ("ats", "ATS", true)]
        );
        assert_eq!(profile.to_json()["schema_version"], 12);
        assert_eq!(profile.to_json()["agents"][0]["source"], agent);
    }

    #[test]
    fn version_one_without_sources_is_rejected_as_no_agent() {
        let error = error_for(|v| {
            v["schema_version"] = json!(1);
            v.as_object_mut().unwrap().remove("agents");
            v["sources"] = json!({});
        });
        assert_eq!(error.path, "agents");
        assert_eq!(error.problem, ProfileProblem::NoAgent);
    }

    #[test]
    fn version_one_errors_keep_version_one_paths() {
        let error = error_for(|v| {
            v["schema_version"] = json!(1);
            v.as_object_mut().unwrap().remove("agents");
            v["sources"] = json!({"boamp": {"keywords": "sso"}});
        });
        assert_eq!(error.path, "sources.boamp.keywords");
    }

    #[test]
    fn contact_local_part_must_not_contain_at() {
        let error = error_for(|v| {
            v["contact_ranking"]["preferred_local_parts"] = json!(["security", "it@example.com"])
        });
        assert_eq!(error.path, "contact_ranking.preferred_local_parts[1]");
        assert_eq!(error.problem, ProfileProblem::InvalidLocalPart);
    }

    #[test]
    fn duplicate_local_parts_are_rejected() {
        let error =
            error_for(|v| v["contact_ranking"]["preferred_local_parts"] = json!(["it", "it"]));
        assert_eq!(error.path, "contact_ranking.preferred_local_parts[1]");
        assert!(matches!(error.problem, ProfileProblem::Duplicate(_)));
    }

    #[test]
    fn guessed_local_parts_must_be_ranked() {
        let error =
            error_for(|v| v["contact_ranking"]["guess_local_parts"] = json!(["contact", "sales"]));
        assert_eq!(error.path, "contact_ranking.guess_local_parts[1]");
        assert_eq!(error.problem, ProfileProblem::GuessNotRanked);
    }

    fn v4() -> Value {
        ValidatedProfile::parse(minimal()).unwrap().to_json()
    }

    fn v4_error(mutate: impl FnOnce(&mut Value)) -> ProfileValidationError {
        let mut value = v4();
        mutate(&mut value);
        ValidatedProfile::parse(value).unwrap_err()
    }

    fn v4_ok(mutate: impl FnOnce(&mut Value)) -> ValidatedProfile {
        let mut value = v4();
        mutate(&mut value);
        ValidatedProfile::parse(value).unwrap()
    }

    fn v8() -> Value {
        let mut value = v4();
        value["schema_version"] = json!(8);
        value.as_object_mut().unwrap().remove("scoring_agents");
        value.as_object_mut().unwrap().remove("calibration");
        value.as_object_mut().unwrap().remove("escalation");
        for agent in value["qualification_agents"].as_array_mut().unwrap() {
            let agent = agent.as_object_mut().unwrap();
            agent.remove("samples");
            agent.remove("sampling_temperature");
        }
        value
    }

    fn v7() -> Value {
        let mut value = v8();
        value["schema_version"] = json!(7);
        for agent in value["qualification_agents"].as_array_mut().unwrap() {
            agent.as_object_mut().unwrap().remove("exclusion_threshold");
            for question in agent["questions"].as_array_mut().unwrap() {
                for option in question["options"].as_array_mut().unwrap() {
                    option.as_object_mut().unwrap().remove("excludes");
                }
            }
        }
        value
    }

    fn v6() -> Value {
        let mut value = v7();
        value["schema_version"] = json!(6);
        for agent in value["qualification_agents"].as_array_mut().unwrap() {
            for question in agent["questions"].as_array_mut().unwrap() {
                let options = question["options"].as_array_mut().unwrap();
                options.pop();
                for option in options {
                    option.as_object_mut().unwrap().remove("is_unknown");
                }
            }
        }
        value
    }

    const OPTION0: &str = "qualification_agents[0].questions[0].options[0]";

    #[test]
    fn version_three_is_upgraded_with_a_default_qualification_agent() {
        let profile = ValidatedProfile::parse(minimal()).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        assert_eq!(definition.qualification_agents.len(), 1);
        let agent = &definition.qualification_agents[0];
        assert_eq!(agent.id.as_str(), "pertinence");
        assert_eq!(agent.name, "Pertinence");
        assert_eq!(
            agent.description,
            "La pertinence du lead pour l'offre du profil."
        );
        assert!(agent.enabled);
        assert_eq!(agent.provider_id, None);
        assert_eq!(agent.weight, 5);
        assert!(agent.system_prompt.contains("Self-hosts an IAM product"));
        assert!(agent.system_prompt.contains("Managed IAM"));
        assert_eq!(agent.questions.len(), 1);
        let question = &agent.questions[0];
        assert_eq!(question.id, "pertinence");
        assert_eq!(question.wording, "Ce lead est-il pertinent pour l'offre ?");
        let options: Vec<_> = question
            .options
            .iter()
            .map(|option| (option.value.as_str(), option.score))
            .collect();
        assert_eq!(
            options,
            [
                ("forte", 1.0),
                ("moyenne", 0.5),
                ("faible", 0.0),
                ("inconnu", 0.5)
            ]
        );
        assert!(question.options[3].is_unknown);
        assert!(
            question.options[..3]
                .iter()
                .all(|option| !option.is_unknown)
        );
        assert_eq!(definition.aggregator.provider_id, None);
        assert!(!definition.aggregator.system_prompt.is_empty());
        assert_eq!(definition.verdict.reliable_from, 70);
        assert_eq!(definition.verdict.discard_below, 30);
        assert_eq!(definition.llm.qualification_threshold, 50);
    }

    #[test]
    fn upgraded_prompt_stays_within_the_limit() {
        let profile = error_for_ok(|v| v["llm"]["relevance_criteria"] = json!("x".repeat(20_000)));
        let prompt = &profile.definition().qualification_agents[0].system_prompt;
        assert_eq!(prompt.chars().count(), 8000);
    }

    #[test]
    fn version_four_round_trips_with_a_null_provider() {
        let value = v4();
        assert_eq!(value["qualification_agents"][0]["provider_id"], Value::Null);
        assert_eq!(value["aggregator"]["provider_id"], Value::Null);
        let profile = ValidatedProfile::parse(value.clone()).unwrap();
        assert_eq!(profile.to_json(), value);
    }

    #[test]
    fn provider_id_must_be_a_uuid() {
        let error = v4_error(|v| v["qualification_agents"][0]["provider_id"] = json!("nope"));
        assert_eq!(error.path, "qualification_agents[0].provider_id");
        let id = "0192f3a0-7b5e-7c1a-9d3e-0a1b2c3d4e5f";
        let profile = v4_ok(|v| v["aggregator"]["provider_id"] = json!(id));
        assert_eq!(
            profile
                .definition()
                .aggregator
                .provider_id
                .unwrap()
                .to_string(),
            id
        );
    }

    #[test]
    fn qualification_agent_is_required() {
        let error = v4_error(|v| v["qualification_agents"] = json!([]));
        assert_eq!(error.path, "qualification_agents");
        assert_eq!(error.problem, ProfileProblem::NoQualificationAgent);
        let error = v4_error(|v| {
            v.as_object_mut().unwrap().remove("qualification_agents");
        });
        assert_eq!(error.path, "$");
    }

    fn v5() -> Value {
        let mut value = v6();
        value["schema_version"] = json!(5);
        let object = value.as_object_mut().unwrap();
        object.remove("evidence");
        object["verdict"] = json!({"reliable_from": 80, "discard_below": 20});
        value
    }

    #[test]
    fn version_five_is_upgraded_with_default_evidence_settings() {
        let profile = ValidatedProfile::parse(v5()).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        assert_eq!(definition.verdict.reliable_from, 80);
        assert_eq!(definition.verdict.discard_below, 20);
        assert_eq!(definition.verdict.calibration_floor, 0.05);
        assert_eq!(definition.verdict.min_evidence, 0.5);
        assert_eq!(definition.evidence, EvidenceSettings::default());
        assert_eq!(definition.evidence.influence, 0.6);
        assert_eq!(definition.evidence.reference_score, 50.0);
        assert_eq!(profile.to_json()["schema_version"], 12);
    }

    #[test]
    fn version_five_rejects_the_evidence_section() {
        let mut value = v5();
        value["evidence"] = json!({});
        let error = ValidatedProfile::parse(value).unwrap_err();
        assert_eq!(error.path, "evidence");
    }

    #[test]
    fn a_version_six_document_without_the_new_sections_gets_defaults() {
        let profile = v4_ok(|v| {
            let object = v.as_object_mut().unwrap();
            object.remove("evidence");
            object["verdict"] = json!({});
        });
        assert_eq!(profile.definition().verdict, VerdictThresholds::default());
        assert_eq!(profile.definition().evidence, EvidenceSettings::default());
    }

    #[test]
    fn calibration_floor_is_between_zero_and_a_fifth() {
        for floor in [0.0, 0.05, 0.2] {
            let _ = v4_ok(|v| v["verdict"]["calibration_floor"] = json!(floor));
        }
        for floor in [-0.01, 0.21, 1.0] {
            let error = v4_error(|v| v["verdict"]["calibration_floor"] = json!(floor));
            assert_eq!(error.path, "verdict.calibration_floor");
            assert_eq!(error.problem, ProfileProblem::InvalidCalibrationFloor);
        }
    }

    #[test]
    fn min_evidence_is_a_fraction() {
        for value in [-0.1, 1.1] {
            let error = v4_error(|v| v["verdict"]["min_evidence"] = json!(value));
            assert_eq!(error.path, "verdict.min_evidence");
            assert_eq!(error.problem, ProfileProblem::InvalidFraction);
        }
        let _ = v4_ok(|v| v["verdict"]["min_evidence"] = json!(1.0));
    }

    #[test]
    fn evidence_influence_is_a_fraction() {
        let error = v4_error(|v| v["evidence"]["influence"] = json!(1.5));
        assert_eq!(error.path, "evidence.influence");
        assert_eq!(error.problem, ProfileProblem::InvalidFraction);
        let _ = v4_ok(|v| v["evidence"]["influence"] = json!(0.0));
    }

    #[test]
    fn the_reference_score_is_at_least_one() {
        let error = v4_error(|v| v["evidence"]["reference_score"] = json!(0.5));
        assert_eq!(error.path, "evidence.reference_score");
        assert_eq!(error.problem, ProfileProblem::InvalidReferenceScore);
        let _ = v4_ok(|v| v["evidence"]["reference_score"] = json!(1.0));
    }

    #[test]
    fn evidence_weights_are_zero_to_ten_with_one_above_zero() {
        let error = v4_error(|v| v["evidence"]["weights"]["volume"] = json!(11));
        assert_eq!(error.path, "evidence.weights.volume");
        assert_eq!(error.problem, ProfileProblem::InvalidEvidenceWeight);
        let error = v4_error(|v| {
            v["evidence"]["weights"] =
                json!({"volume": 0, "diversity": 0, "environment": 0, "strength": 0});
        });
        assert_eq!(error.path, "evidence.weights");
        assert_eq!(error.problem, ProfileProblem::NoEvidenceWeight);
        let _ = v4_ok(|v| {
            v["evidence"]["weights"] =
                json!({"volume": 0, "diversity": 0, "environment": 0, "strength": 3});
        });
    }

    #[test]
    fn half_lives_default_to_the_documented_values() {
        let profile = v4_ok(|v| {
            v["evidence"]
                .as_object_mut()
                .unwrap()
                .remove("half_life_days");
        });
        let evidence = &profile.definition().evidence;
        assert_eq!(evidence.half_life_days_for("job_offer"), 90);
        assert_eq!(evidence.half_life_days_for("public_tender"), 180);
        assert_eq!(evidence.half_life_days_for("awarded_contract"), 365);
        assert_eq!(evidence.half_life_days_for("saas_provider"), 365);
        assert_eq!(evidence.half_life_days_for("product_detected"), 365);
        assert_eq!(evidence.half_life_days_for("debt_marker"), 365);
        assert_eq!(evidence.half_life_days_for("contact"), 365);
        assert_eq!(evidence.half_life_days_for("anything_else"), 180);
    }

    #[test]
    fn a_partial_half_life_map_falls_back_to_built_ins_then_to_default() {
        let profile = v4_ok(|v| {
            v["evidence"]["half_life_days"] = json!({"job_offer": 30, "default": 45});
        });
        let evidence = &profile.definition().evidence;
        assert_eq!(evidence.half_life_days_for("job_offer"), 30);
        assert_eq!(evidence.half_life_days_for("saas_provider"), 365);
        assert_eq!(evidence.half_life_days_for("anything_else"), 45);
    }

    #[test]
    fn half_lives_are_days_from_one_to_3650_of_valid_kinds() {
        for days in [0, 3651] {
            let error = v4_error(|v| v["evidence"]["half_life_days"] = json!({"job_offer": days}));
            assert_eq!(error.path, "evidence.half_life_days.job_offer");
            assert_eq!(error.problem, ProfileProblem::InvalidHalfLife);
        }
        let error = v4_error(|v| v["evidence"]["half_life_days"] = json!({"Job Offer": 30}));
        assert_eq!(error.path, "evidence.half_life_days.Job Offer");
        assert_eq!(error.problem, ProfileProblem::InvalidSignalKind);
        let _ = v4_ok(|v| v["evidence"]["half_life_days"] = json!({"job_offer": 3650, "x": 1}));
    }

    #[test]
    fn version_nine_is_upgraded_with_the_default_half_lives() {
        let mut value = v4();
        value["schema_version"] = json!(9);
        value.as_object_mut().unwrap().remove("scoring_agents");
        value.as_object_mut().unwrap().remove("calibration");
        value["evidence"]
            .as_object_mut()
            .unwrap()
            .remove("half_life_days");
        let profile = ValidatedProfile::parse(value).unwrap();
        assert_eq!(profile.definition().schema_version, 12);
        assert_eq!(profile.definition().evidence, EvidenceSettings::default());
    }

    #[test]
    fn qualification_agent_defaults() {
        let profile = v4_ok(|v| {
            let agent = v["qualification_agents"][0].as_object_mut().unwrap();
            agent.remove("enabled");
            agent.remove("weight");
            agent.remove("provider_id");
            v["qualification_agents"][0]["questions"][0]
                .as_object_mut()
                .unwrap()
                .remove("weight");
        });
        let agent = &profile.definition().qualification_agents[0];
        assert!(agent.enabled);
        assert_eq!(agent.weight, 5);
        assert_eq!(agent.provider_id, None);
        assert_eq!(agent.questions[0].weight, 5);
    }

    #[test]
    fn qualification_agent_description_is_required_and_bounded() {
        let error = v4_error(|v| {
            v["qualification_agents"][0]
                .as_object_mut()
                .unwrap()
                .remove("description");
        });
        assert_eq!(error.path, "qualification_agents[0]");
        let error = v4_error(|v| v["qualification_agents"][0]["description"] = json!(" "));
        assert_eq!(error.path, "qualification_agents[0].description");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error =
            v4_error(|v| v["qualification_agents"][0]["description"] = json!("x".repeat(301)));
        assert_eq!(error.path, "qualification_agents[0].description");
        assert_eq!(error.problem, ProfileProblem::DescriptionTooLong);
        let ok = v4_ok(|v| v["qualification_agents"][0]["description"] = json!("x".repeat(300)));
        assert_eq!(
            ok.definition().qualification_agents[0]
                .description
                .chars()
                .count(),
            300
        );
    }

    #[test]
    fn version_four_documents_get_a_description_derived_from_the_name() {
        let mut value = v6();
        value["schema_version"] = json!(4);
        value.as_object_mut().unwrap().remove("evidence");
        let agent = value["qualification_agents"][0].as_object_mut().unwrap();
        agent.remove("description");
        agent.insert("name".to_owned(), json!("Maturité technique"));
        let profile = ValidatedProfile::parse(value).unwrap();
        let agent = &profile.definition().qualification_agents[0];
        assert_eq!(profile.definition().schema_version, 12);
        assert_eq!(agent.description, "Évalue : Maturité technique");
        assert_eq!(agent.name, "Maturité technique");
        assert_eq!(profile.to_json()["schema_version"], 12);
    }

    #[test]
    fn version_four_upgrade_keeps_the_description_within_the_limit() {
        let mut value = v6();
        value["schema_version"] = json!(4);
        value.as_object_mut().unwrap().remove("evidence");
        let agent = value["qualification_agents"][0].as_object_mut().unwrap();
        agent.remove("description");
        agent.insert("name".to_owned(), json!("n".repeat(500)));
        let profile = ValidatedProfile::parse(value).unwrap();
        let description = &profile.definition().qualification_agents[0].description;
        assert_eq!(description.chars().count(), 300);
    }

    #[test]
    fn qualification_agent_ids_are_slugs() {
        let error = v4_error(|v| v["qualification_agents"][0]["id"] = json!("Bad Id"));
        assert_eq!(error.path, "qualification_agents[0].id");
        assert_eq!(error.problem, ProfileProblem::InvalidAgentId);
    }

    #[test]
    fn ids_are_unique_across_sourcing_and_qualification_agents() {
        let error = v4_error(|v| v["qualification_agents"][0]["id"] = json!("http-probe"));
        assert_eq!(error.path, "qualification_agents[0].id");
        assert_eq!(
            error.problem,
            ProfileProblem::Duplicate("http-probe".to_owned())
        );
        let error = v4_error(|v| {
            let first = v["qualification_agents"][0].clone();
            v["qualification_agents"]
                .as_array_mut()
                .unwrap()
                .push(first);
        });
        assert_eq!(error.path, "qualification_agents[1].id");
        assert_eq!(
            error.problem,
            ProfileProblem::Duplicate("pertinence".to_owned())
        );
    }

    #[test]
    fn qualification_agent_fields_are_checked_with_their_paths() {
        let error = v4_error(|v| v["qualification_agents"][0]["name"] = json!(" "));
        assert_eq!(error.path, "qualification_agents[0].name");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error = v4_error(|v| v["qualification_agents"][0]["system_prompt"] = json!(" "));
        assert_eq!(error.path, "qualification_agents[0].system_prompt");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error =
            v4_error(|v| v["qualification_agents"][0]["system_prompt"] = json!("x".repeat(8001)));
        assert_eq!(error.problem, ProfileProblem::TooLong);
        let ok = v4_ok(|v| v["qualification_agents"][0]["system_prompt"] = json!("x".repeat(8000)));
        assert_eq!(
            ok.definition().qualification_agents[0].system_prompt.len(),
            8000
        );
        for weight in [0, 11] {
            let error = v4_error(|v| v["qualification_agents"][0]["weight"] = json!(weight));
            assert_eq!(error.path, "qualification_agents[0].weight");
            assert_eq!(error.problem, ProfileProblem::InvalidWeight);
        }
    }

    #[test]
    fn at_least_one_question_is_required() {
        let error = v4_error(|v| v["qualification_agents"][0]["questions"] = json!([]));
        assert_eq!(error.path, "qualification_agents[0].questions");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn question_fields_are_checked_with_their_paths() {
        let error =
            v4_error(|v| v["qualification_agents"][0]["questions"][0]["id"] = json!("Bad Id"));
        assert_eq!(error.path, "qualification_agents[0].questions[0].id");
        assert_eq!(error.problem, ProfileProblem::InvalidIdentifier);
        let error =
            v4_error(|v| v["qualification_agents"][0]["questions"][0]["wording"] = json!(" "));
        assert_eq!(error.path, "qualification_agents[0].questions[0].wording");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error =
            v4_error(|v| v["qualification_agents"][0]["questions"][0]["weight"] = json!(11));
        assert_eq!(error.path, "qualification_agents[0].questions[0].weight");
        assert_eq!(error.problem, ProfileProblem::InvalidWeight);
    }

    #[test]
    fn question_ids_are_unique_within_an_agent() {
        let error = v4_error(|v| {
            let first = v["qualification_agents"][0]["questions"][0].clone();
            v["qualification_agents"][0]["questions"]
                .as_array_mut()
                .unwrap()
                .push(first);
        });
        assert_eq!(error.path, "qualification_agents[0].questions[1].id");
        assert_eq!(
            error.problem,
            ProfileProblem::Duplicate("pertinence".to_owned())
        );
    }

    #[test]
    fn options_count_is_between_two_and_eleven() {
        let options = |count: usize| {
            (0..count)
                .map(|i| {
                    json!({"value": format!("o{i}"), "label": "L", "score": 0.5, "is_unknown": i == 0})
                })
                .collect::<Vec<_>>()
        };
        for count in [0, 1, 12] {
            let error = v4_error(|v| {
                v["qualification_agents"][0]["questions"][0]["options"] = json!(options(count))
            });
            assert_eq!(error.path, "qualification_agents[0].questions[0].options");
            assert_eq!(error.problem, ProfileProblem::InvalidOptionCount);
        }
        for count in [2, 11] {
            let _ = v4_ok(|v| {
                v["qualification_agents"][0]["questions"][0]["options"] = json!(options(count))
            });
        }
    }

    #[test]
    fn option_values_are_unique_identifiers() {
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][1]["value"] = json!("forte")
        });
        assert_eq!(
            error.path,
            format!("{OPTION0}.value").replace("[0].value", "[1].value")
        );
        assert_eq!(error.problem, ProfileProblem::Duplicate("forte".to_owned()));
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][0]["value"] = json!("A b")
        });
        assert_eq!(error.path, format!("{OPTION0}.value"));
        assert_eq!(error.problem, ProfileProblem::InvalidIdentifier);
        let _ = v4_ok(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][0]["value"] =
                json!("legacy_auto_heberge")
        });
    }

    #[test]
    fn option_scores_are_between_zero_and_one() {
        for score in [json!(1.5), json!(-0.1)] {
            let error = v4_error(|v| {
                v["qualification_agents"][0]["questions"][0]["options"][0]["score"] = score
            });
            assert_eq!(error.path, format!("{OPTION0}.score"));
            assert_eq!(error.problem, ProfileProblem::InvalidScore);
        }
        for score in [json!(0), json!(1), json!(0.3)] {
            let _ = v4_ok(|v| {
                v["qualification_agents"][0]["questions"][0]["options"][0]["score"] = score
            });
        }
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][0]["label"] = json!(" ")
        });
        assert_eq!(error.path, format!("{OPTION0}.label"));
    }

    #[test]
    fn aggregator_prompt_is_required_and_bounded() {
        let error = v4_error(|v| v["aggregator"]["system_prompt"] = json!(""));
        assert_eq!(error.path, "aggregator.system_prompt");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error = v4_error(|v| v["aggregator"]["system_prompt"] = json!("x".repeat(8001)));
        assert_eq!(error.problem, ProfileProblem::TooLong);
    }

    #[test]
    fn verdict_thresholds_default_and_are_ordered() {
        let profile = v4_ok(|v| {
            v.as_object_mut().unwrap().remove("verdict");
        });
        assert_eq!(profile.definition().verdict.reliable_from, 70);
        assert_eq!(profile.definition().verdict.discard_below, 30);
        let error = v4_error(|v| v["verdict"] = json!({"reliable_from": 30, "discard_below": 30}));
        assert_eq!(error.path, "verdict.reliable_from");
        assert_eq!(error.problem, ProfileProblem::ThresholdOrder);
        let error = v4_error(|v| v["verdict"] = json!({"reliable_from": 101}));
        assert_eq!(error.path, "verdict.reliable_from");
        assert_eq!(error.problem, ProfileProblem::InvalidThreshold);
        let error =
            v4_error(|v| v["verdict"] = json!({"reliable_from": 100, "discard_below": 101}));
        assert_eq!(error.path, "verdict.discard_below");
        let _ = v4_ok(|v| v["verdict"] = json!({"reliable_from": 1, "discard_below": 0}));
    }

    #[test]
    fn version_four_rejects_unknown_fields_in_new_objects() {
        let error = v4_error(|v| v["aggregator"]["extra"] = json!(1));
        assert_eq!(error.path, "aggregator.extra");
        let error = v4_error(|v| v["qualification_agents"][0]["questions"][0]["x"] = json!(1));
        assert_eq!(error.path, "qualification_agents[0].questions[0].x");
    }

    fn options_of(profile: &ValidatedProfile) -> Vec<(String, f64, bool)> {
        profile.definition().qualification_agents[0].questions[0]
            .options
            .iter()
            .map(|option| (option.value.clone(), option.score, option.is_unknown))
            .collect()
    }

    #[test]
    fn version_six_reuses_an_existing_unknown_option() {
        for value in ["inconnu", "inconnue", "unknown"] {
            let mut document = v6();
            document["qualification_agents"][0]["questions"][0]["options"]
                .as_array_mut()
                .unwrap()
                .insert(
                    1,
                    json!({"value": value, "label": "Je ne sais pas", "score": 0.4}),
                );
            let profile = ValidatedProfile::parse(document).unwrap();
            assert_eq!(profile.definition().schema_version, 12);
            assert_eq!(
                options_of(&profile),
                [
                    ("forte".to_owned(), 1.0, false),
                    (value.to_owned(), 0.4, true),
                    ("moyenne".to_owned(), 0.5, false),
                    ("faible".to_owned(), 0.0, false),
                ]
            );
            let kept = &profile.definition().qualification_agents[0].questions[0].options[1];
            assert_eq!(kept.label, "Je ne sais pas");
        }
    }

    #[test]
    fn version_six_appends_an_unknown_option_when_missing() {
        let profile = ValidatedProfile::parse(v6()).unwrap();
        assert_eq!(
            options_of(&profile),
            [
                ("forte".to_owned(), 1.0, false),
                ("moyenne".to_owned(), 0.5, false),
                ("faible".to_owned(), 0.0, false),
                ("inconnu".to_owned(), 0.5, true),
            ]
        );
        let appended = &profile.definition().qualification_agents[0].questions[0].options[3];
        assert_eq!(appended.label, "Inconnu");
        assert_eq!(profile.to_json()["schema_version"], 12);
    }

    #[test]
    fn version_six_with_ten_options_still_validates_after_the_append() {
        let mut document = v6();
        document["qualification_agents"][0]["questions"][0]["options"] = json!(
            (0..10)
                .map(|i| json!({"value": format!("o{i}"), "label": "L", "score": 0.5}))
                .collect::<Vec<_>>()
        );
        let profile = ValidatedProfile::parse(document).unwrap();
        assert_eq!(options_of(&profile).len(), 11);
        assert!(options_of(&profile)[10].2);
    }

    #[test]
    fn older_versions_get_an_unknown_option_on_every_axis() {
        for version in [4, 5] {
            let mut document = v5();
            document["schema_version"] = json!(version);
            if version == 4 {
                document.as_object_mut().unwrap().remove("evidence");
                document["qualification_agents"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("description");
            }
            let profile = ValidatedProfile::parse(document).unwrap();
            assert!(options_of(&profile)[3].2);
        }
    }

    #[test]
    fn version_seven_is_upgraded_with_exclusion_defaults() {
        let profile = ValidatedProfile::parse(v7()).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        for agent in &definition.qualification_agents {
            assert_eq!(agent.exclusion_threshold, 0.6);
            assert!(
                agent
                    .questions
                    .iter()
                    .flat_map(|question| &question.options)
                    .all(|option| !option.excludes)
            );
        }
        assert_eq!(profile.to_json()["schema_version"], 12);
        assert_eq!(profile, ValidatedProfile::parse(v4()).unwrap());
    }

    #[test]
    fn version_seven_rejects_the_exclusion_fields() {
        let mut document = v7();
        document["qualification_agents"][0]["questions"][0]["options"][0]["excludes"] = json!(true);
        let error = ValidatedProfile::parse(document).unwrap_err();
        assert_eq!(error.path, format!("{OPTION0}.excludes"));
        let mut document = v7();
        document["qualification_agents"][0]["exclusion_threshold"] = json!(0.7);
        let error = ValidatedProfile::parse(document).unwrap_err();
        assert_eq!(error.path, "qualification_agents[0].exclusion_threshold");
    }

    #[test]
    fn version_eight_is_upgraded_with_sampling_and_escalation_defaults() {
        let profile = ValidatedProfile::parse(v8()).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        assert_eq!(definition.escalation, Escalation::default());
        assert_eq!(definition.escalation.provider, None);
        assert_eq!(
            (
                definition.escalation.band_low,
                definition.escalation.band_high
            ),
            (40, 70)
        );
        for agent in &definition.qualification_agents {
            assert_eq!(agent.samples, 3);
            assert_eq!(agent.sampling_temperature, 0.7);
        }
        assert_eq!(profile.to_json()["schema_version"], 12);
        assert_eq!(profile, ValidatedProfile::parse(v4()).unwrap());
    }

    #[test]
    fn version_eight_rejects_the_sampling_and_escalation_fields() {
        let mut document = v8();
        document["qualification_agents"][0]["samples"] = json!(2);
        let error = ValidatedProfile::parse(document).unwrap_err();
        assert_eq!(error.path, "qualification_agents[0].samples");
        let mut document = v8();
        document["escalation"] = json!({});
        let error = ValidatedProfile::parse(document).unwrap_err();
        assert_eq!(error.path, "escalation");
    }

    #[test]
    fn sampling_defaults_apply_when_the_fields_are_missing() {
        let profile = v4_ok(|v| {
            let agent = v["qualification_agents"][0].as_object_mut().unwrap();
            agent.remove("samples");
            agent.remove("sampling_temperature");
            v.as_object_mut().unwrap().remove("escalation");
        });
        let agent = &profile.definition().qualification_agents[0];
        assert_eq!((agent.samples, agent.sampling_temperature), (3, 0.7));
        assert_eq!(profile.definition().escalation, Escalation::default());
    }

    #[test]
    fn samples_are_between_one_and_five() {
        for bad in [json!(0), json!(6), json!(255)] {
            let error = v4_error(|v| v["qualification_agents"][0]["samples"] = bad);
            assert_eq!(error.path, "qualification_agents[0].samples");
            assert_eq!(error.problem, ProfileProblem::InvalidSamples);
        }
        for good in [1, 3, 5] {
            let profile = v4_ok(|v| v["qualification_agents"][0]["samples"] = json!(good));
            assert_eq!(profile.definition().qualification_agents[0].samples, good);
        }
    }

    #[test]
    fn the_sampling_temperature_is_between_zero_and_one_and_a_half() {
        for bad in [json!(-0.1), json!(1.51), json!(2)] {
            let error = v4_error(|v| v["qualification_agents"][0]["sampling_temperature"] = bad);
            assert_eq!(error.path, "qualification_agents[0].sampling_temperature");
            assert_eq!(error.problem, ProfileProblem::InvalidSamplingTemperature);
        }
        for good in [json!(0), json!(0.7), json!(1.5)] {
            let profile = v4_ok(|v| v["qualification_agents"][0]["sampling_temperature"] = good);
            assert!(profile.definition().qualification_agents[0].sampling_temperature <= 1.5);
        }
    }

    #[test]
    fn the_grey_band_must_be_ordered_and_a_percentage() {
        for (low, high) in [(70, 70), (80, 40), (0, 0)] {
            let error = v4_error(|v| {
                v["escalation"]["band_low"] = json!(low);
                v["escalation"]["band_high"] = json!(high);
            });
            assert_eq!(error.path, "escalation.band_high");
            assert_eq!(error.problem, ProfileProblem::BandOrder);
        }
        let error = v4_error(|v| v["escalation"]["band_high"] = json!(101));
        assert_eq!(error.path, "escalation.band_high");
        assert_eq!(error.problem, ProfileProblem::InvalidThreshold);
        let profile = v4_ok(|v| {
            v["escalation"]["band_low"] = json!(0);
            v["escalation"]["band_high"] = json!(100);
            v["escalation"]["provider"] = json!("0192f0f0-0000-7000-8000-000000000001");
        });
        assert!(profile.definition().escalation.provider.is_some());
    }

    #[test]
    fn escalation_triggers_only_inside_the_band_with_a_provider() {
        let enabled = Escalation {
            provider: Some(uuid::Uuid::nil()),
            ..Escalation::default()
        };
        assert!(!enabled.triggers(39.9));
        assert!(enabled.triggers(40.0));
        assert!(enabled.triggers(69.9));
        assert!(!enabled.triggers(70.0));
        assert!(!Escalation::default().triggers(55.0));
    }

    #[test]
    fn the_exclusion_threshold_defaults_to_sixty_percent() {
        let profile = v4_ok(|v| {
            v["qualification_agents"][0]
                .as_object_mut()
                .unwrap()
                .remove("exclusion_threshold");
        });
        assert_eq!(
            profile.definition().qualification_agents[0].exclusion_threshold,
            0.6
        );
    }

    #[test]
    fn the_exclusion_threshold_is_between_half_and_one() {
        for bad in [json!(0.49), json!(1.01), json!(0.0), json!(-1)] {
            let error = v4_error(|v| v["qualification_agents"][0]["exclusion_threshold"] = bad);
            assert_eq!(error.path, "qualification_agents[0].exclusion_threshold");
            assert_eq!(error.problem, ProfileProblem::InvalidExclusionThreshold);
        }
        for good in [json!(0.5), json!(0.75), json!(1.0)] {
            let profile = v4_ok(|v| v["qualification_agents"][0]["exclusion_threshold"] = good);
            assert!(profile.definition().qualification_agents[0].exclusion_threshold >= 0.5);
        }
    }

    #[test]
    fn an_option_can_exclude_but_not_the_unknown_one() {
        let profile = v4_ok(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][0]["excludes"] = json!(true)
        });
        assert!(profile.definition().qualification_agents[0].questions[0].options[0].excludes);
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][3]["excludes"] = json!(true)
        });
        assert_eq!(
            error.path,
            "qualification_agents[0].questions[0].options[3].excludes"
        );
        assert_eq!(error.problem, ProfileProblem::UnknownCannotExclude);
    }

    #[test]
    fn an_upgraded_profile_is_stable_when_parsed_again() {
        let profile = ValidatedProfile::parse(v6()).unwrap();
        let again = ValidatedProfile::parse(profile.to_json()).unwrap();
        assert_eq!(profile, again);
    }

    #[test]
    fn version_six_rejects_the_unknown_flag() {
        let mut document = v6();
        document["qualification_agents"][0]["questions"][0]["options"][0]["is_unknown"] =
            json!(true);
        let error = ValidatedProfile::parse(document).unwrap_err();
        assert_eq!(error.path, format!("{OPTION0}.is_unknown"));
    }

    #[test]
    fn exactly_one_option_per_axis_is_unknown() {
        let options = "qualification_agents[0].questions[0].options";
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][3]["is_unknown"] = json!(false)
        });
        assert_eq!(error.path, options);
        assert_eq!(error.problem, ProfileProblem::UnknownOptionCount);
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][0]["is_unknown"] = json!(true)
        });
        assert_eq!(error.path, options);
        assert_eq!(error.problem, ProfileProblem::UnknownOptionCount);
        let error = v4_error(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][3]
                .as_object_mut()
                .unwrap()
                .remove("is_unknown");
        });
        assert_eq!(error.problem, ProfileProblem::UnknownOptionCount);
    }

    #[test]
    fn the_unknown_option_score_is_editable() {
        let profile = v4_ok(|v| {
            v["qualification_agents"][0]["questions"][0]["options"][3]["score"] = json!(0.2)
        });
        assert_eq!(options_of(&profile)[3], ("inconnu".to_owned(), 0.2, true));
    }

    #[test]
    fn display_prefixes_the_path() {
        let error = error_for(|v| v["identity"]["offer"] = json!(""));
        assert_eq!(error.to_string(), "identity.offer: must not be empty");
    }

    #[test]
    fn version_ten_is_upgraded_with_the_default_scoring_agent() {
        let mut value = v4();
        value["schema_version"] = json!(10);
        value.as_object_mut().unwrap().remove("scoring_agents");
        value.as_object_mut().unwrap().remove("calibration");
        let profile = ValidatedProfile::parse(value).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        assert_eq!(definition.scoring_agents.len(), 1);
        let agent = &definition.scoring_agents[0];
        assert_eq!(agent.id.as_str(), "score-global");
        assert_eq!(agent.name, "Score global");
        assert_eq!(agent.weight, 5);
        assert_eq!(agent.samples, 1);
        assert_eq!(agent.provider_id, None);
        assert!(agent.enabled);
        let ids: Vec<&str> = agent.criteria.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "adequation_offre",
                "solidite_preuves",
                "priorite_commerciale"
            ]
        );
        for criterion in &agent.criteria {
            assert!(criterion.unknown_option().is_some());
            assert!(criterion.options.iter().all(|option| !option.excludes));
        }
        assert!(agent.system_prompt.starts_with("Tu décides"));
    }

    #[test]
    fn version_eleven_is_upgraded_with_learned_calibration_on() {
        let mut value = v4();
        value["schema_version"] = json!(11);
        value.as_object_mut().unwrap().remove("calibration");
        let profile = ValidatedProfile::parse(value).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.schema_version, 12);
        assert!(definition.calibration.learned);
        assert_eq!(profile.to_json()["calibration"]["learned"], json!(true));
    }

    #[test]
    fn learned_calibration_can_be_switched_off() {
        let profile = v4_ok(|v| v["calibration"] = json!({"learned": false}));
        assert!(!profile.definition().calibration.learned);
        let error = v4_error(|v| v["calibration"] = json!({"learnt": false}));
        assert_eq!(error.path, "calibration.learnt");
    }

    #[test]
    fn a_missing_calibration_defaults_to_learned() {
        let profile = v4_ok(|v| {
            v.as_object_mut().unwrap().remove("calibration");
        });
        assert!(profile.definition().calibration.learned);
    }

    #[test]
    fn every_older_version_gets_the_default_scoring_agent() {
        for value in [v6(), v7(), v8()] {
            let profile = ValidatedProfile::parse(value).unwrap();
            assert_eq!(profile.definition().scoring_agents.len(), 1);
        }
        let profile = ValidatedProfile::parse(minimal()).unwrap();
        assert_eq!(profile.definition().scoring_agents.len(), 1);
    }

    #[test]
    fn a_profile_without_scoring_agents_is_valid() {
        let profile = v4_ok(|v| v["scoring_agents"] = json!([]));
        assert!(profile.definition().scoring_agents.is_empty());
        let profile = v4_ok(|v| {
            v.as_object_mut().unwrap().remove("scoring_agents");
        });
        assert!(profile.definition().scoring_agents.is_empty());
    }

    #[test]
    fn scoring_agent_defaults() {
        let profile = v4_ok(|v| {
            let agent = v["scoring_agents"][0].as_object_mut().unwrap();
            agent.remove("enabled");
            agent.remove("weight");
            agent.remove("provider_id");
            agent.remove("samples");
            agent.remove("sampling_temperature");
        });
        let agent = &profile.definition().scoring_agents[0];
        assert!(agent.enabled);
        assert_eq!(agent.weight, 5);
        assert_eq!(agent.samples, 1);
        assert_eq!(agent.sampling_temperature, 0.7);
    }

    #[test]
    fn scoring_agent_ids_are_slugs_unique_across_every_kind() {
        let error = v4_error(|v| v["scoring_agents"][0]["id"] = json!("Bad Id"));
        assert_eq!(error.path, "scoring_agents[0].id");
        assert_eq!(error.problem, ProfileProblem::InvalidAgentId);
        let error = v4_error(|v| {
            let id = v["qualification_agents"][0]["id"].clone();
            v["scoring_agents"][0]["id"] = id;
        });
        assert_eq!(error.path, "scoring_agents[0].id");
        assert!(matches!(error.problem, ProfileProblem::Duplicate(_)));
        let error = v4_error(|v| {
            let first = v["scoring_agents"][0].clone();
            v["scoring_agents"].as_array_mut().unwrap().push(first);
        });
        assert_eq!(error.path, "scoring_agents[1].id");
    }

    #[test]
    fn scoring_agent_weight_samples_and_temperature_are_bounded() {
        for weight in [0, 11] {
            let error = v4_error(|v| v["scoring_agents"][0]["weight"] = json!(weight));
            assert_eq!(error.path, "scoring_agents[0].weight");
            assert_eq!(error.problem, ProfileProblem::InvalidWeight);
        }
        for samples in [0, 6] {
            let error = v4_error(|v| v["scoring_agents"][0]["samples"] = json!(samples));
            assert_eq!(error.path, "scoring_agents[0].samples");
            assert_eq!(error.problem, ProfileProblem::InvalidSamples);
        }
        let error = v4_error(|v| v["scoring_agents"][0]["sampling_temperature"] = json!(1.6));
        assert_eq!(error.path, "scoring_agents[0].sampling_temperature");
        assert_eq!(error.problem, ProfileProblem::InvalidSamplingTemperature);
        let _ = v4_ok(|v| v["scoring_agents"][0]["samples"] = json!(5));
    }

    #[test]
    fn scoring_agent_text_fields_are_bounded() {
        let error = v4_error(|v| v["scoring_agents"][0]["description"] = json!("x".repeat(301)));
        assert_eq!(error.problem, ProfileProblem::DescriptionTooLong);
        let error = v4_error(|v| v["scoring_agents"][0]["system_prompt"] = json!(" "));
        assert_eq!(error.path, "scoring_agents[0].system_prompt");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error = v4_error(|v| v["scoring_agents"][0]["name"] = json!(""));
        assert_eq!(error.path, "scoring_agents[0].name");
    }

    #[test]
    fn scoring_criteria_follow_the_question_rules() {
        let error = v4_error(|v| v["scoring_agents"][0]["criteria"] = json!([]));
        assert_eq!(error.path, "scoring_agents[0].criteria");
        assert_eq!(error.problem, ProfileProblem::Empty);
        let error = v4_error(|v| v["scoring_agents"][0]["criteria"][0]["id"] = json!("Bad Id"));
        assert_eq!(error.path, "scoring_agents[0].criteria[0].id");
        assert_eq!(error.problem, ProfileProblem::InvalidIdentifier);
        let error = v4_error(|v| {
            let first = v["scoring_agents"][0]["criteria"][0].clone();
            v["scoring_agents"][0]["criteria"]
                .as_array_mut()
                .unwrap()
                .push(first);
        });
        assert_eq!(error.path, "scoring_agents[0].criteria[3].id");
        let error = v4_error(|v| {
            for option in v["scoring_agents"][0]["criteria"][0]["options"]
                .as_array_mut()
                .unwrap()
            {
                option["is_unknown"] = json!(false);
            }
        });
        assert_eq!(error.problem, ProfileProblem::UnknownOptionCount);
    }

    #[test]
    fn a_scoring_criterion_cannot_exclude() {
        let error = v4_error(|v| {
            v["scoring_agents"][0]["criteria"][1]["options"][0]["excludes"] = json!(true);
        });
        assert_eq!(
            error.path,
            "scoring_agents[0].criteria[1].options[0].excludes"
        );
        assert_eq!(error.problem, ProfileProblem::CriterionCannotExclude);
    }
}
