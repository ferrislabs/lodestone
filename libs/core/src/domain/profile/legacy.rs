use serde::Deserialize;
use uuid::Uuid;

use super::definition::{
    Agent, AgentId, AgentSource, Aggregator, CertificateTransparency, ContactRanking,
    DEFAULT_EXCLUSION_THRESHOLD, DnsCname, EvidenceSettings, HttpProbe, Identity, KeywordSource,
    Language, LlmSettings, MAX_DESCRIPTION_CHARS, MAX_PROMPT_CHARS, PROFILE_SCHEMA_VERSION,
    ProfileDefinition, QualificationAgent, Question, QuestionOption, Scoring, ScoringFactor,
    VerdictThresholds,
};
use crate::domain::scoring::max_possible;

const FALLBACK_THRESHOLD_PERCENT: u8 = 50;
const TOP_WEIGHT: f64 = 10.0;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FactorV2 {
    signal: String,
    #[serde(default)]
    value: Option<String>,
    weight: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScoringV2 {
    factors: Vec<FactorV2>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LlmV2 {
    qualification_threshold: f64,
    relevance_criteria: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV2 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    scoring: ScoringV2,
    contact_ranking: ContactRanking,
    llm: LlmV2,
}

fn keeps_weights(factors: &[FactorV2]) -> bool {
    factors
        .iter()
        .all(|factor| factor.weight.fract() == 0.0 && (1.0..=TOP_WEIGHT).contains(&factor.weight))
}

fn rescale(weight: f64, largest: f64) -> u8 {
    let scaled = if largest > 0.0 {
        (weight * TOP_WEIGHT / largest).round()
    } else {
        1.0
    };
    let bounded = scaled.clamp(1.0, TOP_WEIGHT);
    (1..=10u8)
        .find(|candidate| f64::from(*candidate) >= bounded)
        .unwrap_or(10)
}

fn upgrade_factors(factors: &[FactorV2]) -> Vec<ScoringFactor> {
    let keep = keeps_weights(factors);
    let largest = factors
        .iter()
        .map(|factor| factor.weight)
        .fold(f64::MIN, f64::max);
    factors
        .iter()
        .map(|factor| ScoringFactor {
            signal: factor.signal.clone(),
            value: factor.value.clone(),
            weight: if keep {
                rescale(factor.weight, TOP_WEIGHT)
            } else {
                rescale(factor.weight, largest)
            },
        })
        .collect()
}

fn upgrade_threshold(points: f64, factors: &[FactorV2]) -> u8 {
    let old_max = max_possible(
        factors
            .iter()
            .map(|factor| (factor.signal.as_str(), factor.weight)),
    );
    if old_max <= 0.0 {
        return FALLBACK_THRESHOLD_PERCENT;
    }
    let percent = (points / old_max * 100.0).round().clamp(0.0, 100.0);
    (0..=100u8)
        .find(|candidate| f64::from(*candidate) >= percent)
        .unwrap_or(100)
}

fn upgrade_scoring_and_llm(scoring: ScoringV2, llm: LlmV2) -> (Scoring, LlmSettings) {
    let threshold = upgrade_threshold(llm.qualification_threshold, &scoring.factors);
    let factors = upgrade_factors(&scoring.factors);
    (
        Scoring { factors },
        LlmSettings {
            qualification_threshold: threshold,
            relevance_criteria: llm.relevance_criteria,
            language: Language::default(),
        },
    )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV3 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
}

const TRIAGE_INTENT: &str = "Tu examines des documents publics sur une entreprise pour un outil de détection de leads B2B. Détermine si ces documents montrent que cette entreprise est un prospect pertinent pour l'offre ci-dessous, en t'appuyant sur les critères de pertinence. Base tes réponses uniquement sur ce que disent les documents. Ne suppose aucun fait qui n'est pas écrit.";

const AGGREGATOR_PROMPT: &str = "Tu résumes l'évaluation d'un lead pour un outil de détection de leads B2B. Tu reçois le lead, ses signaux ainsi que les réponses et les scores de chaque agent de qualification. Rédige une courte synthèse, le récit du verdict, un angle d'approche et un brouillon de message. Cite les preuves mot pour mot depuis les documents. Ne modifie jamais la confiance ni le verdict calculés. Base ton texte uniquement sur ce qui est fourni.";

fn pertinence_prompt(identity: &Identity, llm: &LlmSettings) -> String {
    let full = format!(
        "{TRIAGE_INTENT}\n\nOffre : {}\nClient cible : {}\nCritères de pertinence : {}",
        identity.offer.trim(),
        identity.target_customer.trim(),
        llm.relevance_criteria.trim()
    );
    full.chars().take(MAX_PROMPT_CHARS).collect()
}

fn option(value: &str, label: &str, score: f64) -> QuestionOption {
    QuestionOption {
        value: value.to_owned(),
        label: label.to_owned(),
        score,
        is_unknown: false,
        excludes: false,
    }
}

fn pertinence_agent(identity: &Identity, llm: &LlmSettings) -> QualificationAgent {
    QualificationAgent {
        id: AgentId::new("pertinence"),
        name: "Pertinence".to_owned(),
        description: "La pertinence du lead pour l'offre du profil.".to_owned(),
        enabled: true,
        provider_id: None,
        system_prompt: pertinence_prompt(identity, llm),
        weight: 5,
        exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
        questions: vec![Question {
            id: "pertinence".to_owned(),
            wording: "Ce lead est-il pertinent pour l'offre ?".to_owned(),
            weight: 5,
            options: vec![
                option("forte", "Forte", 1.0),
                option("moyenne", "Moyenne", 0.5),
                option("faible", "Faible", 0.0),
                QuestionOption::unknown(),
            ],
        }],
    }
}

fn default_enabled() -> bool {
    true
}

fn default_weight() -> u8 {
    5
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QualificationAgentV4 {
    id: AgentId,
    name: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    provider_id: Option<Uuid>,
    system_prompt: String,
    #[serde(default = "default_weight")]
    weight: u8,
    questions: Vec<QuestionV6>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionOptionV6 {
    value: String,
    label: String,
    score: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionV6 {
    id: String,
    wording: String,
    #[serde(default = "default_weight")]
    weight: u8,
    options: Vec<QuestionOptionV6>,
}

const UNKNOWN_VALUES: [&str; 3] = ["inconnu", "inconnue", "unknown"];

impl QuestionV6 {
    fn upgrade(self) -> Question {
        let marked = self
            .options
            .iter()
            .position(|option| UNKNOWN_VALUES.contains(&option.value.as_str()));
        let mut options: Vec<QuestionOption> = self
            .options
            .into_iter()
            .enumerate()
            .map(|(index, option)| QuestionOption {
                value: option.value,
                label: option.label,
                score: option.score,
                is_unknown: marked == Some(index),
                excludes: false,
            })
            .collect();
        if marked.is_none() {
            options.push(QuestionOption::unknown());
        }
        Question {
            id: self.id,
            wording: self.wording,
            weight: self.weight,
            options,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QualificationAgentV6 {
    id: AgentId,
    name: String,
    description: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    provider_id: Option<Uuid>,
    system_prompt: String,
    #[serde(default = "default_weight")]
    weight: u8,
    questions: Vec<QuestionV6>,
}

impl QualificationAgentV6 {
    fn upgrade(self) -> QualificationAgent {
        QualificationAgent {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            weight: self.weight,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            questions: self
                .questions
                .into_iter()
                .map(QuestionV6::upgrade)
                .collect(),
        }
    }
}

impl QualificationAgentV4 {
    fn upgrade(self) -> QualificationAgent {
        let description: String = format!("Évalue : {}", self.name.trim())
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        QualificationAgent {
            id: self.id,
            name: self.name,
            description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            weight: self.weight,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            questions: self
                .questions
                .into_iter()
                .map(QuestionV6::upgrade)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV4 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    qualification_agents: Vec<QualificationAgentV4>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
}

impl ProfileDefinitionV4 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV4::upgrade)
                .collect(),
            aggregator: self.aggregator,
            verdict: self.verdict,
            evidence: EvidenceSettings::default(),
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV5 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    qualification_agents: Vec<QualificationAgentV6>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
}

impl ProfileDefinitionV5 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV6::upgrade)
                .collect(),
            aggregator: self.aggregator,
            verdict: self.verdict,
            evidence: EvidenceSettings::default(),
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV6 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    qualification_agents: Vec<QualificationAgentV6>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceSettings,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
}

impl ProfileDefinitionV6 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV6::upgrade)
                .collect(),
            aggregator: self.aggregator,
            verdict: self.verdict,
            evidence: self.evidence,
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionOptionV7 {
    value: String,
    label: String,
    score: f64,
    #[serde(default)]
    is_unknown: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionV7 {
    id: String,
    wording: String,
    #[serde(default = "default_weight")]
    weight: u8,
    options: Vec<QuestionOptionV7>,
}

impl QuestionV7 {
    fn upgrade(self) -> Question {
        Question {
            id: self.id,
            wording: self.wording,
            weight: self.weight,
            options: self
                .options
                .into_iter()
                .map(|option| QuestionOption {
                    value: option.value,
                    label: option.label,
                    score: option.score,
                    is_unknown: option.is_unknown,
                    excludes: false,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QualificationAgentV7 {
    id: AgentId,
    name: String,
    description: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    provider_id: Option<Uuid>,
    system_prompt: String,
    #[serde(default = "default_weight")]
    weight: u8,
    questions: Vec<QuestionV7>,
}

impl QualificationAgentV7 {
    fn upgrade(self) -> QualificationAgent {
        QualificationAgent {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            weight: self.weight,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            questions: self
                .questions
                .into_iter()
                .map(QuestionV7::upgrade)
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV7 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<Agent>,
    qualification_agents: Vec<QualificationAgentV7>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceSettings,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
}

impl ProfileDefinitionV7 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV7::upgrade)
                .collect(),
            aggregator: self.aggregator,
            verdict: self.verdict,
            evidence: self.evidence,
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}

impl ProfileDefinitionV3 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let qualification_agents = vec![pertinence_agent(&self.identity, &self.llm)];
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            qualification_agents,
            aggregator: Aggregator {
                provider_id: None,
                system_prompt: AGGREGATOR_PROMPT.to_owned(),
            },
            verdict: VerdictThresholds::default(),
            evidence: EvidenceSettings::default(),
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}

impl ProfileDefinitionV2 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let (scoring, llm) = upgrade_scoring_and_llm(self.scoring, self.llm);
        ProfileDefinitionV3 {
            _schema_version: 3,
            identity: self.identity,
            agents: self.agents,
            scoring,
            contact_ranking: self.contact_ranking,
            llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV1 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    sources: SourcesV1,
    scoring: ScoringV2,
    contact_ranking: ContactRanking,
    llm: LlmV2,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesV1 {
    #[serde(default)]
    certificate_transparency: Option<CertificateTransparency>,
    #[serde(default)]
    http_probe: Option<HttpProbe>,
    #[serde(default)]
    dns_cname: Option<DnsCname>,
    #[serde(default)]
    boamp: Option<KeywordSource>,
    #[serde(default)]
    decp: Option<KeywordSource>,
    #[serde(default)]
    ted: Option<KeywordSource>,
    #[serde(default)]
    france_travail: Option<KeywordSource>,
    #[serde(default)]
    ats: Option<KeywordSource>,
}

fn agent(id: &str, name: &str, source: AgentSource) -> Agent {
    Agent {
        id: AgentId::new(id),
        name: name.to_owned(),
        enabled: true,
        source,
        llm: None,
    }
}

impl SourcesV1 {
    fn into_agents(self) -> Vec<Agent> {
        let mut agents = Vec::new();
        if let Some(rules) = self.certificate_transparency {
            agents.push(agent(
                "certificate-transparency",
                "Certificate transparency",
                AgentSource::CertificateTransparency(rules),
            ));
        }
        if let Some(rules) = self.http_probe {
            agents.push(agent(
                "http-probe",
                "HTTP probe",
                AgentSource::HttpProbe(rules),
            ));
        }
        if let Some(rules) = self.dns_cname {
            agents.push(agent(
                "dns-cname",
                "DNS CNAME",
                AgentSource::DnsCname(rules),
            ));
        }
        if let Some(rules) = self.boamp {
            agents.push(agent("boamp", "BOAMP", AgentSource::Boamp(rules)));
        }
        if let Some(rules) = self.decp {
            agents.push(agent("decp", "DECP", AgentSource::Decp(rules)));
        }
        if let Some(rules) = self.ted {
            agents.push(agent("ted", "TED", AgentSource::Ted(rules)));
        }
        if let Some(rules) = self.france_travail {
            agents.push(agent(
                "france-travail",
                "France Travail",
                AgentSource::FranceTravail(rules),
            ));
        }
        if let Some(rules) = self.ats {
            agents.push(agent("ats", "ATS", AgentSource::Ats(rules)));
        }
        agents
    }
}

impl ProfileDefinitionV1 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let (scoring, llm) = upgrade_scoring_and_llm(self.scoring, self.llm);
        ProfileDefinitionV3 {
            _schema_version: 3,
            identity: self.identity,
            agents: self.sources.into_agents(),
            scoring,
            contact_ranking: self.contact_ranking,
            llm,
        }
        .upgrade()
    }
}
