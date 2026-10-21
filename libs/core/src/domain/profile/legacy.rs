use std::collections::BTreeMap;

use serde::{Deserialize, de::IgnoredAny};
use uuid::Uuid;

use super::{
    definition::{
        AgentId, Aggregator, CertificateTransparency, ContactRanking, DEFAULT_EXCLUSION_THRESHOLD,
        DEFAULT_SAMPLES, DEFAULT_SAMPLING_TEMPERATURE, DEFAULT_SCORING_SAMPLES, DnsCname,
        Escalation, EvidenceSettings, EvidenceWeights, HttpProbe, Identity, Language,
        MAX_DESCRIPTION_CHARS, MAX_PROMPT_CHARS, ProfileDefinition, QualificationAgent, Question,
        QuestionOption, ScoringAgent, VerdictThresholds, default_exclusion_threshold,
        default_half_life_days, default_influence, default_samples, default_sampling_temperature,
    },
    v15::{AgentSourceV15, AgentV15, KeywordSourceV15, ProfileDefinitionV15},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentLlmV13 {
    #[serde(default)]
    relevance_criteria: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentV13 {
    id: AgentId,
    name: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    source: AgentSourceV15,
    #[serde(default)]
    llm: Option<AgentLlmV13>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LlmV13 {
    #[serde(rename = "qualification_threshold")]
    _qualification_threshold: IgnoredAny,
    relevance_criteria: String,
    #[serde(default)]
    language: Language,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CalibrationV13 {
    #[serde(default = "default_enabled")]
    learned: bool,
}

impl Default for CalibrationV13 {
    fn default() -> Self {
        Self { learned: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LlmV2 {
    #[serde(rename = "qualification_threshold")]
    _qualification_threshold: IgnoredAny,
    relevance_criteria: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV2 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV2,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceLegacy {
    #[serde(default = "default_influence")]
    influence: f64,
    #[serde(default, rename = "reference_score")]
    _reference_score: IgnoredAny,
    #[serde(default)]
    weights: EvidenceWeights,
    #[serde(default = "default_half_life_days")]
    half_life_days: BTreeMap<String, u32>,
}

impl Default for EvidenceLegacy {
    fn default() -> Self {
        Self {
            influence: default_influence(),
            _reference_score: IgnoredAny,
            weights: EvidenceWeights::default(),
            half_life_days: default_half_life_days(),
        }
    }
}

impl From<EvidenceLegacy> for EvidenceSettings {
    fn from(legacy: EvidenceLegacy) -> Self {
        Self {
            influence: legacy.influence,
            weights: legacy.weights,
            half_life_days: legacy.half_life_days,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV3 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

const TRIAGE_INTENT: &str = "Tu examines des documents publics sur une entreprise pour un outil de détection de leads B2B. Détermine si ces documents montrent que cette entreprise est un prospect pertinent pour l'offre ci-dessous, en t'appuyant sur les critères de pertinence. Base tes réponses uniquement sur ce que disent les documents. Ne suppose aucun fait qui n'est pas écrit.";

const SCORING_PROMPT: &str = "Tu décides du score et du verdict d'un lead pour un outil de détection de leads B2B. Tu reçois le lead, ses faits, ses signaux, le dossier de l'entreprise, le rapport de chaque agent de qualification (résumé, réponses avec citations, accord entre échantillons, exclusions) et des indicateurs calculés par le système. Pèse l'ensemble comme le ferait un commercial prudent. Réponds à chaque critère en citant mot pour mot une phrase des documents, ou choisis l'option inconnue quand rien ne la soutient. Donne un score de 0 à 100, la probabilité de chaque verdict et quelques raisons courtes. Base-toi uniquement sur ce qui est fourni.";

const AGGREGATOR_PROMPT: &str = "Tu résumes l'évaluation d'un lead pour un outil de détection de leads B2B. Tu reçois le lead, ses signaux ainsi que les réponses et les scores de chaque agent de qualification. Rédige une courte synthèse, le récit du verdict, un angle d'approche et un brouillon de message. Cite les preuves mot pour mot depuis les documents. Ne modifie jamais la confiance ni le verdict calculés. Base ton texte uniquement sur ce qui est fourni.";

fn pertinence_prompt(identity: &Identity, llm: &LlmV13) -> String {
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

fn pertinence_agent(identity: &Identity, llm: &LlmV13) -> QualificationAgentV14 {
    QualificationAgentV14 {
        id: AgentId::new("pertinence"),
        name: "Pertinence".to_owned(),
        description: "La pertinence du lead pour l'offre du profil.".to_owned(),
        enabled: true,
        provider_id: None,
        system_prompt: pertinence_prompt(identity, llm),
        _weight: IgnoredAny,
        exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
        samples: DEFAULT_SAMPLES,
        sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
        learned_calibration: true,
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

fn criterion(id: &str, wording: &str, options: [(&str, &str, f64); 3]) -> Question {
    let mut listed: Vec<QuestionOption> = options
        .iter()
        .map(|(value, label, score)| option(value, label, *score))
        .collect();
    listed.push(QuestionOption::unknown());
    Question {
        id: id.to_owned(),
        wording: wording.to_owned(),
        weight: 5,
        options: listed,
    }
}

pub(super) fn default_scoring_agents() -> Vec<ScoringAgent> {
    vec![ScoringAgent {
        id: AgentId::new("score-global"),
        name: "Score global".to_owned(),
        description: "Décide du score et du verdict du lead à partir des rapports des agents de qualification.".to_owned(),
        enabled: true,
        provider_id: None,
        weight: 5,
        system_prompt: SCORING_PROMPT.to_owned(),
        criteria: vec![
            criterion(
                "adequation_offre",
                "Le lead correspond-il à la cible de l'offre ?",
                [
                    ("forte", "Forte", 1.0),
                    ("moyenne", "Moyenne", 0.5),
                    ("faible", "Faible", 0.0),
                ],
            ),
            criterion(
                "solidite_preuves",
                "Les preuves rassemblées sont-elles solides et récentes ?",
                [
                    ("solide", "Solide", 1.0),
                    ("partielle", "Partielle", 0.5),
                    ("fragile", "Fragile", 0.0),
                ],
            ),
            criterion(
                "priorite_commerciale",
                "Ce lead mérite-t-il d'être contacté en priorité ?",
                [
                    ("haute", "Haute", 1.0),
                    ("normale", "Normale", 0.5),
                    ("basse", "Basse", 0.0),
                ],
            ),
        ],
        samples: DEFAULT_SCORING_SAMPLES,
        sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
        learned_calibration: true,
    }]
}

fn default_enabled() -> bool {
    true
}

fn default_weight() -> u8 {
    5
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct QualificationAgentV14 {
    id: AgentId,
    name: String,
    description: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    provider_id: Option<Uuid>,
    system_prompt: String,
    #[serde(default, rename = "weight")]
    _weight: IgnoredAny,
    #[serde(default = "default_exclusion_threshold")]
    exclusion_threshold: f64,
    #[serde(default = "default_samples")]
    samples: u8,
    #[serde(default = "default_sampling_temperature")]
    sampling_temperature: f64,
    questions: Vec<Question>,
    #[serde(default = "default_enabled")]
    learned_calibration: bool,
}

impl QualificationAgentV14 {
    fn upgrade(self) -> QualificationAgent {
        QualificationAgent {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            exclusion_threshold: self.exclusion_threshold,
            samples: self.samples,
            sampling_temperature: self.sampling_temperature,
            questions: self.questions,
            learned_calibration: self.learned_calibration,
        }
    }
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
    #[serde(default, rename = "weight")]
    _weight: IgnoredAny,
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
    #[serde(default, rename = "weight")]
    _weight: IgnoredAny,
    questions: Vec<QuestionV6>,
}

impl QualificationAgentV6 {
    fn upgrade(self) -> QualificationAgentV14 {
        QualificationAgentV14 {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            _weight: IgnoredAny,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            samples: DEFAULT_SAMPLES,
            sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
            learned_calibration: true,
            questions: self
                .questions
                .into_iter()
                .map(QuestionV6::upgrade)
                .collect(),
        }
    }
}

impl QualificationAgentV4 {
    fn upgrade(self) -> QualificationAgentV14 {
        let description: String = format!("Évalue : {}", self.name.trim())
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect();
        QualificationAgentV14 {
            id: self.id,
            name: self.name,
            description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            _weight: IgnoredAny,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            samples: DEFAULT_SAMPLES,
            sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
            learned_calibration: true,
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
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV4>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV4 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV4::upgrade)
                .collect(),
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: Escalation::default(),
            verdict: self.verdict,
            evidence: EvidenceSettings::default(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV5 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV6>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV5 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV6::upgrade)
                .collect(),
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: Escalation::default(),
            verdict: self.verdict,
            evidence: EvidenceSettings::default(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV6 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV6>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV6 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV6::upgrade)
                .collect(),
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: Escalation::default(),
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
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
    #[serde(default, rename = "weight")]
    _weight: IgnoredAny,
    questions: Vec<QuestionV7>,
}

impl QualificationAgentV7 {
    fn upgrade(self) -> QualificationAgentV14 {
        QualificationAgentV14 {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            _weight: IgnoredAny,
            exclusion_threshold: DEFAULT_EXCLUSION_THRESHOLD,
            samples: DEFAULT_SAMPLES,
            sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
            learned_calibration: true,
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
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV7>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV7 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV7::upgrade)
                .collect(),
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: Escalation::default(),
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QualificationAgentV8 {
    id: AgentId,
    name: String,
    description: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    #[serde(default)]
    provider_id: Option<Uuid>,
    system_prompt: String,
    #[serde(default, rename = "weight")]
    _weight: IgnoredAny,
    #[serde(default = "default_exclusion_threshold")]
    exclusion_threshold: f64,
    questions: Vec<Question>,
}

impl QualificationAgentV8 {
    fn upgrade(self) -> QualificationAgentV14 {
        QualificationAgentV14 {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled: self.enabled,
            provider_id: self.provider_id,
            system_prompt: self.system_prompt,
            _weight: IgnoredAny,
            exclusion_threshold: self.exclusion_threshold,
            samples: DEFAULT_SAMPLES,
            sampling_temperature: DEFAULT_SAMPLING_TEMPERATURE,
            learned_calibration: true,
            questions: self.questions,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV8 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV8>,
    aggregator: Aggregator,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV8 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV8::upgrade)
                .collect(),
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: Escalation::default(),
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

impl ProfileDefinitionV3 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let qualification_agents = vec![pertinence_agent(&self.identity, &self.llm)];
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents,
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: Aggregator {
                provider_id: None,
                system_prompt: AGGREGATOR_PROMPT.to_owned(),
            },
            escalation: Escalation::default(),
            verdict: VerdictThresholds::default(),
            evidence: EvidenceSettings::default(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

impl ProfileDefinitionV2 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let llm = LlmV13 {
            _qualification_threshold: IgnoredAny,
            relevance_criteria: self.llm.relevance_criteria,
            language: Language::default(),
        };
        ProfileDefinitionV3 {
            _schema_version: 3,
            identity: self.identity,
            agents: self.agents,
            _scoring: IgnoredAny,
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
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
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
    boamp: Option<KeywordSourceV15>,
    #[serde(default)]
    decp: Option<KeywordSourceV15>,
    #[serde(default)]
    ted: Option<KeywordSourceV15>,
    #[serde(default)]
    france_travail: Option<KeywordSourceV15>,
    #[serde(default)]
    ats: Option<KeywordSourceV15>,
}

fn agent(id: &str, name: &str, source: AgentSourceV15) -> AgentV13 {
    AgentV13 {
        id: AgentId::new(id),
        name: name.to_owned(),
        enabled: true,
        source,
        llm: None,
    }
}

impl SourcesV1 {
    fn into_agents(self) -> Vec<AgentV13> {
        let mut agents = Vec::new();
        if let Some(rules) = self.certificate_transparency {
            agents.push(agent(
                "certificate-transparency",
                "Certificate transparency",
                AgentSourceV15::CertificateTransparency(rules),
            ));
        }
        if let Some(rules) = self.http_probe {
            agents.push(agent(
                "http-probe",
                "HTTP probe",
                AgentSourceV15::HttpProbe(rules),
            ));
        }
        if let Some(rules) = self.dns_cname {
            agents.push(agent(
                "dns-cname",
                "DNS CNAME",
                AgentSourceV15::DnsCname(rules),
            ));
        }
        if let Some(rules) = self.boamp {
            agents.push(agent("boamp", "BOAMP", AgentSourceV15::Boamp(rules)));
        }
        if let Some(rules) = self.decp {
            agents.push(agent("decp", "DECP", AgentSourceV15::Decp(rules)));
        }
        if let Some(rules) = self.ted {
            agents.push(agent("ted", "TED", AgentSourceV15::Ted(rules)));
        }
        if let Some(rules) = self.france_travail {
            agents.push(agent(
                "france-travail",
                "France Travail",
                AgentSourceV15::FranceTravail(rules),
            ));
        }
        if let Some(rules) = self.ats {
            agents.push(agent("ats", "ATS", AgentSourceV15::Ats(rules)));
        }
        agents
    }
}

impl ProfileDefinitionV1 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let llm = LlmV13 {
            _qualification_threshold: IgnoredAny,
            relevance_criteria: self.llm.relevance_criteria,
            language: Language::default(),
        };
        ProfileDefinitionV3 {
            _schema_version: 3,
            identity: self.identity,
            agents: self.sources.into_agents(),
            _scoring: IgnoredAny,
            contact_ranking: self.contact_ranking,
            llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV10 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV14>,
    aggregator: Aggregator,
    #[serde(default)]
    escalation: Escalation,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV10 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self.qualification_agents,
            scoring_agents: default_scoring_agents(),
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: self.escalation,
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV11 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV14>,
    #[serde(default)]
    scoring_agents: Vec<ScoringAgent>,
    aggregator: Aggregator,
    #[serde(default)]
    escalation: Escalation,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV11 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self.qualification_agents,
            scoring_agents: self.scoring_agents,
            calibration: CalibrationV13::default(),
            aggregator: self.aggregator,
            escalation: self.escalation,
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV12 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV14>,
    #[serde(default)]
    scoring_agents: Vec<ScoringAgent>,
    aggregator: Aggregator,
    #[serde(default)]
    escalation: Escalation,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceLegacy,
    #[serde(default)]
    calibration: CalibrationV13,
    #[serde(rename = "scoring")]
    _scoring: IgnoredAny,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

impl ProfileDefinitionV12 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV13 {
            _schema_version: 13,
            identity: self.identity,
            agents: self.agents,
            qualification_agents: self.qualification_agents,
            scoring_agents: self.scoring_agents,
            calibration: self.calibration,
            aggregator: self.aggregator,
            escalation: self.escalation,
            verdict: self.verdict,
            evidence: self.evidence.into(),
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV13 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    agents: Vec<AgentV13>,
    qualification_agents: Vec<QualificationAgentV14>,
    #[serde(default)]
    scoring_agents: Vec<ScoringAgent>,
    aggregator: Aggregator,
    #[serde(default)]
    escalation: Escalation,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceSettings,
    #[serde(default)]
    calibration: CalibrationV13,
    contact_ranking: ContactRanking,
    llm: LlmV13,
}

fn normalised(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn append_criteria(prompt: &mut String, prefix: &str, criteria: &str) {
    let criteria = criteria.trim();
    if criteria.is_empty() || normalised(prompt).contains(&normalised(criteria)) {
        return;
    }
    let used = prompt.chars().count();
    let addition = format!("\n\n{prefix} : {criteria}");
    let prefix_chars = addition.chars().count() - criteria.chars().count();
    let room = MAX_PROMPT_CHARS.saturating_sub(used);
    if room <= prefix_chars {
        return;
    }
    prompt.extend(addition.chars().take(room));
}

impl ProfileDefinitionV13 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let Self {
            identity,
            agents,
            qualification_agents,
            mut scoring_agents,
            aggregator,
            escalation,
            verdict,
            evidence,
            calibration,
            contact_ranking,
            llm,
            ..
        } = self;
        let mut qualification_agents: Vec<QualificationAgent> = qualification_agents
            .into_iter()
            .map(QualificationAgentV14::upgrade)
            .collect();
        if let Some(first) = qualification_agents.first_mut() {
            append_criteria(
                &mut first.system_prompt,
                "Critères de pertinence",
                &llm.relevance_criteria,
            );
            for agent in &agents {
                if let Some(criteria) = agent
                    .llm
                    .as_ref()
                    .and_then(|llm| llm.relevance_criteria.as_deref())
                {
                    let prefix = format!("Critères de l'agent « {} »", agent.name.trim());
                    append_criteria(&mut first.system_prompt, &prefix, criteria);
                }
            }
        }
        for agent in &mut qualification_agents {
            agent.learned_calibration = calibration.learned;
        }
        for agent in &mut scoring_agents {
            agent.learned_calibration = calibration.learned;
        }
        ProfileDefinitionV15 {
            _schema_version: 15,
            identity,
            language: llm.language,
            agents: agents
                .into_iter()
                .map(|agent| AgentV15 {
                    id: agent.id,
                    name: agent.name,
                    enabled: agent.enabled,
                    source: agent.source,
                })
                .collect(),
            qualification_agents,
            scoring_agents,
            aggregator,
            escalation,
            verdict,
            evidence,
            contact_ranking,
        }
        .upgrade()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV14 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    #[serde(default)]
    language: Language,
    agents: Vec<AgentV15>,
    qualification_agents: Vec<QualificationAgentV14>,
    #[serde(default)]
    scoring_agents: Vec<ScoringAgent>,
    aggregator: Aggregator,
    #[serde(default)]
    escalation: Escalation,
    #[serde(default)]
    verdict: VerdictThresholds,
    #[serde(default)]
    evidence: EvidenceSettings,
    contact_ranking: ContactRanking,
}

impl ProfileDefinitionV14 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV15 {
            _schema_version: 15,
            identity: self.identity,
            language: self.language,
            agents: self.agents,
            qualification_agents: self
                .qualification_agents
                .into_iter()
                .map(QualificationAgentV14::upgrade)
                .collect(),
            scoring_agents: self.scoring_agents,
            aggregator: self.aggregator,
            escalation: self.escalation,
            verdict: self.verdict,
            evidence: self.evidence,
            contact_ranking: self.contact_ranking,
        }
        .upgrade()
    }
}
