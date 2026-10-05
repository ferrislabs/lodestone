use serde::Deserialize;

use super::definition::{
    Agent, AgentId, AgentSource, CertificateTransparency, ContactRanking, DnsCname, HttpProbe,
    Identity, KeywordSource, LlmSettings, PROFILE_SCHEMA_VERSION, ProfileDefinition, Scoring,
    ScoringFactor,
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
        },
    )
}

impl ProfileDefinitionV2 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let (scoring, llm) = upgrade_scoring_and_llm(self.scoring, self.llm);
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.agents,
            scoring,
            contact_ranking: self.contact_ranking,
            llm,
        }
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
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.sources.into_agents(),
            scoring,
            contact_ranking: self.contact_ranking,
            llm,
        }
    }
}
