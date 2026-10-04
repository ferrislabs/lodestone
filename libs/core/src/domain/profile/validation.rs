use std::{collections::HashSet, fmt};

use regex::RegexBuilder;
use serde::Serialize;
use thiserror::Error;

use super::definition::{
    CertificateTransparency, CnamePattern, ContactRanking, DebtRule, DnsCname, Fingerprint,
    HttpProbe, Identity, KeywordSource, LlmSettings, Matcher, PROFILE_SCHEMA_VERSION,
    ProfileDefinition, Scoring, Sources, VersionExtractor,
};

const REGEX_SIZE_LIMIT: usize = 1 << 20;
const MAX_PATTERN_LENGTH: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProfileProblem {
    #[error("malformed: {0}")]
    Malformed(String),
    #[error("unsupported schema version {found}, expected {PROFILE_SCHEMA_VERSION}")]
    UnsupportedSchemaVersion { found: u32 },
    #[error("must not be empty")]
    Empty,
    #[error("duplicate value `{0}`")]
    Duplicate(String),
    #[error("no source is enabled")]
    NoSourceEnabled,
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
        let definition: ProfileDefinition =
            serde_path_to_error::deserialize(value).map_err(|error| ProfileValidationError {
                path: path_or_root(error.path().to_string()),
                problem: ProfileProblem::Malformed(error.inner().to_string()),
            })?;
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
    validate_sources(&definition.sources, &root.field("sources"))?;
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

fn validate_sources(sources: &Sources, path: &Path) -> Check {
    let keyword_sources = [
        ("boamp", &sources.boamp),
        ("decp", &sources.decp),
        ("ted", &sources.ted),
        ("france_travail", &sources.france_travail),
        ("ats", &sources.ats),
    ];
    let any_enabled = sources.certificate_transparency.is_some()
        || sources.http_probe.is_some()
        || sources.dns_cname.is_some()
        || keyword_sources.iter().any(|(_, source)| source.is_some());
    if !any_enabled {
        return Err(path.fail(ProfileProblem::NoSourceEnabled));
    }
    if let Some(ct) = &sources.certificate_transparency {
        validate_ct(ct, &path.field("certificate_transparency"))?;
    }
    if let Some(probe) = &sources.http_probe {
        validate_http_probe(probe, &path.field("http_probe"))?;
    }
    if let Some(dns) = &sources.dns_cname {
        validate_dns(dns, &path.field("dns_cname"))?;
    }
    for (name, source) in keyword_sources {
        if let Some(source) = source {
            validate_keyword_source(source, &path.field(name))?;
        }
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
    use crate::domain::profile::fixtures::MINIMAL_PROFILE;

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

    #[test]
    fn unsupported_schema_version_is_rejected() {
        let error = error_for(|v| v["schema_version"] = json!(2));
        assert_eq!(error.path, "schema_version");
        assert_eq!(
            error.problem,
            ProfileProblem::UnsupportedSchemaVersion { found: 2 }
        );
    }

    #[test]
    fn empty_offer_is_rejected() {
        let error = error_for(|v| v["identity"]["offer"] = json!("  "));
        assert_eq!(error.path, "identity.offer");
        assert_eq!(error.problem, ProfileProblem::Empty);
    }

    #[test]
    fn profile_without_sources_is_rejected() {
        let error = error_for(|v| v["sources"] = json!({}));
        assert_eq!(error.path, "sources");
        assert_eq!(error.problem, ProfileProblem::NoSourceEnabled);
    }

    #[test]
    fn invalid_regex_points_at_the_pattern() {
        let error = error_for(|v| {
            v["sources"]["http_probe"]["fingerprints"][0]["matchers"][1]["pattern"] = json!("(")
        });
        assert_eq!(
            error.path,
            "sources.http_probe.fingerprints[0].matchers[1].pattern"
        );
        assert_eq!(error.problem, ProfileProblem::InvalidPattern);
    }

    #[test]
    fn version_extractor_needs_a_capture_group() {
        let error = error_for(|v| {
            v["sources"]["http_probe"]["fingerprints"][0]["version"]["pattern"] = json!("^\\d+")
        });
        assert_eq!(
            error.path,
            "sources.http_probe.fingerprints[0].version.pattern"
        );
        assert_eq!(error.problem, ProfileProblem::MissingCaptureGroup);
    }

    #[test]
    fn version_below_needs_an_extractor() {
        let error = error_for(|v| {
            v["sources"]["http_probe"]["fingerprints"][0]
                .as_object_mut()
                .unwrap()
                .remove("version");
        });
        assert_eq!(
            error.path,
            "sources.http_probe.fingerprints[0].debt_markers[0].rule"
        );
        assert_eq!(error.problem, ProfileProblem::VersionBelowWithoutExtractor);
    }

    #[test]
    fn fingerprint_path_must_be_absolute() {
        let error = error_for(|v| {
            v["sources"]["http_probe"]["fingerprints"][0]["path"] = json!("https://evil.test/x")
        });
        assert_eq!(error.path, "sources.http_probe.fingerprints[0].path");
        assert_eq!(error.problem, ProfileProblem::InvalidPath);
    }

    #[test]
    fn duplicate_fingerprint_ids_are_rejected() {
        let error = error_for(|v| {
            let first = v["sources"]["http_probe"]["fingerprints"][0].clone();
            v["sources"]["http_probe"]["fingerprints"]
                .as_array_mut()
                .unwrap()
                .push(first);
        });
        assert_eq!(error.path, "sources.http_probe.fingerprints[1].id");
        assert!(matches!(error.problem, ProfileProblem::Duplicate(_)));
    }

    #[test]
    fn ct_labels_must_be_hostname_labels() {
        let error = error_for(|v| {
            v["sources"]["certificate_transparency"] = json!({"labels": ["sso", "Auth Login"]})
        });
        assert_eq!(error.path, "sources.certificate_transparency.labels[1]");
        assert_eq!(error.problem, ProfileProblem::InvalidLabel);
    }

    #[test]
    fn cname_suffix_must_be_a_domain_suffix() {
        let error = error_for(|v| {
            v["sources"]["dns_cname"] =
                json!({"patterns": [{"provider": "auth0", "target_suffix": "auth0"}]})
        });
        assert_eq!(error.path, "sources.dns_cname.patterns[0].target_suffix");
        assert_eq!(error.problem, ProfileProblem::InvalidSuffix);
    }

    #[test]
    fn keyword_source_needs_keywords() {
        let error = error_for(|v| v["sources"]["boamp"] = json!({"keywords": []}));
        assert_eq!(error.path, "sources.boamp.keywords");
        assert_eq!(error.problem, ProfileProblem::Empty);
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
