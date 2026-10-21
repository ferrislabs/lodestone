use serde::Deserialize;

use super::{
    definition::{
        AgentId, Aggregator, CertificateTransparency, ContactRanking, DnsCname, Escalation,
        EvidenceSettings, HttpProbe, Identity, Language, ProfileDefinition, QualificationAgent,
        ScoringAgent, SearchAgent, SearchFilters, VerdictThresholds, default_accept_min_keywords,
    },
    v16::{AgentV16, ProfileDefinitionV16, SignalAgentV16, SignalSourceV16},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeywordSourceV15 {
    keywords: Vec<String>,
    #[serde(default)]
    exclusions: Vec<String>,
    #[serde(default = "default_accept_min_keywords")]
    accept_min_keywords: u8,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum AgentSourceV15 {
    CertificateTransparency(CertificateTransparency),
    DnsCname(DnsCname),
    HttpProbe(HttpProbe),
    Boamp(KeywordSourceV15),
    Decp(KeywordSourceV15),
    Ted(KeywordSourceV15),
    FranceTravail(KeywordSourceV15),
    Ats(KeywordSourceV15),
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentV15 {
    pub(super) id: AgentId,
    pub(super) name: String,
    #[serde(default = "default_enabled")]
    pub(super) enabled: bool,
    pub(super) source: AgentSourceV15,
}

impl AgentV15 {
    fn upgrade(self) -> AgentV16 {
        let Self {
            id,
            name,
            enabled,
            source,
        } = self;
        let (provider, rules) = match source {
            AgentSourceV15::CertificateTransparency(rules) => {
                return signal(
                    id,
                    name,
                    enabled,
                    SignalSourceV16::CertificateTransparency(rules),
                );
            }
            AgentSourceV15::DnsCname(rules) => {
                return signal(id, name, enabled, SignalSourceV16::DnsCname(rules));
            }
            AgentSourceV15::HttpProbe(rules) => {
                return signal(id, name, enabled, SignalSourceV16::HttpProbe(rules));
            }
            AgentSourceV15::Boamp(rules) => ("boamp", rules),
            AgentSourceV15::Decp(rules) => ("decp", rules),
            AgentSourceV15::Ted(rules) => ("ted", rules),
            AgentSourceV15::FranceTravail(rules) => ("france_travail", rules),
            AgentSourceV15::Ats(rules) => ("ats", rules),
        };
        AgentV16::Search(SearchAgent {
            id,
            name,
            enabled,
            providers: vec![provider.to_owned()],
            keywords: rules.keywords,
            exclusions: rules.exclusions,
            filters: SearchFilters::default(),
            accept_min_keywords: rules.accept_min_keywords,
        })
    }
}

fn signal(id: AgentId, name: String, enabled: bool, source: SignalSourceV16) -> AgentV16 {
    AgentV16::Signal(SignalAgentV16 {
        id,
        name,
        enabled,
        source,
    })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV15 {
    #[serde(rename = "schema_version")]
    pub(super) _schema_version: u32,
    pub(super) identity: Identity,
    #[serde(default)]
    pub(super) language: Language,
    pub(super) agents: Vec<AgentV15>,
    pub(super) qualification_agents: Vec<QualificationAgent>,
    #[serde(default)]
    pub(super) scoring_agents: Vec<ScoringAgent>,
    pub(super) aggregator: Aggregator,
    #[serde(default)]
    pub(super) escalation: Escalation,
    #[serde(default)]
    pub(super) verdict: VerdictThresholds,
    #[serde(default)]
    pub(super) evidence: EvidenceSettings,
    pub(super) contact_ranking: ContactRanking,
}

impl ProfileDefinitionV15 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinitionV16 {
            _schema_version: 16,
            identity: self.identity,
            language: self.language,
            agents: self.agents.into_iter().map(AgentV15::upgrade).collect(),
            qualification_agents: self.qualification_agents,
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
