use serde::Deserialize;

use super::definition::{
    Agent, AgentId, AgentSource, CertificateTransparency, ContactRanking, DnsCname, HttpProbe,
    Identity, KeywordSource, LlmSettings, PROFILE_SCHEMA_VERSION, ProfileDefinition, Scoring,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV1 {
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    identity: Identity,
    sources: SourcesV1,
    scoring: Scoring,
    contact_ranking: ContactRanking,
    llm: LlmSettings,
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
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            agents: self.sources.into_agents(),
            scoring: self.scoring,
            contact_ranking: self.contact_ranking,
            llm: self.llm,
        }
    }
}
