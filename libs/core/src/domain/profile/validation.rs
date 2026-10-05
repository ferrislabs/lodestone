use std::{collections::HashSet, fmt};

use regex::RegexBuilder;
use serde::Serialize;
use thiserror::Error;

use super::{
    definition::{
        Agent, AgentSource, CertificateTransparency, CnamePattern, ContactRanking, DebtRule,
        DnsCname, Fingerprint, HttpProbe, Identity, KeywordSource, LlmSettings, Matcher,
        PROFILE_SCHEMA_VERSION, ProfileDefinition, Scoring, VersionExtractor,
    },
    legacy::ProfileDefinitionV1,
};

const REGEX_SIZE_LIMIT: usize = 1 << 20;
const MAX_PATTERN_LENGTH: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProfileProblem {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("unsupported schema version {found}, expected 1 or {PROFILE_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion { found: u32 },
    #[error("must not be empty")]
    Empty,
    #[error("duplicate value `{0}`")]
    Duplicate(String),
    #[error("at least one agent is required")]
    NoAgent,
    #[error("must be a slug of lowercase letters, digits and hyphens, at most 63 characters")]
    InvalidAgentId,
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
    #[error("must be a finite number")]
    NotFinite,
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

fn finite(value: f64, path: &Path) -> Check {
    if value.is_finite() {
        Ok(())
    } else {
        Err(path.fail(ProfileProblem::NotFinite))
    }
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
        finite(factor.weight, &factor_path.field("weight"))?;
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
    finite(
        llm.qualification_threshold,
        &path.field("qualification_threshold"),
    )?;
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
        let error = error_for(|v| v["schema_version"] = json!(3));
        assert_eq!(error.path, "schema_version");
        assert_eq!(
            error.problem,
            ProfileProblem::UnsupportedSchemaVersion { found: 3 }
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
        assert_eq!(profile.definition().boamp().unwrap().accept_min_keywords, 2);
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

    #[test]
    fn compat_helpers_pick_the_first_enabled_agent_of_the_kind() {
        let profile = ValidatedProfile::parse(two_ats_agents(true)).unwrap();
        let definition = profile.definition();
        assert_eq!(definition.ats().unwrap().keywords, ["iam"]);
        assert!(definition.enables(SourceName::Ats));
        assert!(definition.http_probe().is_some());
        assert!(definition.boamp().is_none());
        assert!(!definition.enables(SourceName::Boamp));
    }

    #[test]
    fn compat_helpers_skip_disabled_agents() {
        let profile = ValidatedProfile::parse(two_ats_agents(false)).unwrap();
        assert_eq!(profile.definition().ats().unwrap().keywords, ["sso"]);
    }

    #[test]
    fn disabled_only_kind_is_not_enabled() {
        let mut value = minimal();
        value["agents"][0]["enabled"] = json!(false);
        let profile = ValidatedProfile::parse(value).unwrap();
        assert!(!profile.definition().enables(SourceName::HttpProbe));
        assert!(profile.definition().http_probe().is_none());
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
        assert_eq!(definition.schema_version, 2);
        let summary: Vec<_> = definition
            .agents
            .iter()
            .map(|agent| (agent.id.as_str(), agent.name.as_str(), agent.enabled))
            .collect();
        assert_eq!(
            summary,
            [("http-probe", "HTTP probe", true), ("ats", "ATS", true)]
        );
        assert_eq!(profile.to_json()["schema_version"], 2);
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

    #[test]
    fn display_prefixes_the_path() {
        let error = error_for(|v| v["identity"]["offer"] = json!(""));
        assert_eq!(error.to_string(), "identity.offer: must not be empty");
    }
}
