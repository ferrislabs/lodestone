use serde::Deserialize;

use super::definition::{
    Agent, AgentId, Aggregator, CertificateTransparency, CertificatesAgent, CompanyJobsEnrichment,
    ContactRanking, DnsAgent, DnsCname, Enrichment, Escalation, EvidenceSettings, HttpProbe,
    HttpProbeEnrichment, Identity, Language, PROFILE_SCHEMA_VERSION, ProfileDefinition,
    QualificationAgent, ScoringAgent, SearchAgent, VerdictThresholds,
};
use crate::domain::source_provider::{
    catalog::{ATS, descriptor},
    descriptor::ProviderCategory,
};

const JOBS_AND_MISSIONS_NAME: &str = "Offres et missions";
const PUBLIC_PROCUREMENT_NAME: &str = "Marchés publics";

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub(super) enum SignalSourceV16 {
    CertificateTransparency(CertificateTransparency),
    DnsCname(DnsCname),
    HttpProbe(HttpProbe),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SignalAgentV16 {
    pub(super) id: AgentId,
    pub(super) name: String,
    #[serde(default = "default_enabled")]
    pub(super) enabled: bool,
    pub(super) source: SignalSourceV16,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum AgentV16 {
    Search(SearchAgent),
    Signal(SignalAgentV16),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV16 {
    #[serde(rename = "schema_version")]
    pub(super) _schema_version: u32,
    pub(super) identity: Identity,
    #[serde(default)]
    pub(super) language: Language,
    pub(super) agents: Vec<AgentV16>,
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

fn merge_probe(into: &mut HttpProbeEnrichment, probe: HttpProbe, enabled: bool) {
    into.enabled |= enabled;
    for fingerprint in probe.fingerprints {
        if !into
            .fingerprints
            .iter()
            .any(|known| known.id == fingerprint.id)
        {
            into.fingerprints.push(fingerprint);
        }
    }
    for product in probe.excluded_products {
        if !into.excluded_products.contains(&product) {
            into.excluded_products.push(product);
        }
    }
}

fn merge_company_jobs(into: &mut CompanyJobsEnrichment, agent: &SearchAgent, first: bool) {
    into.enabled |= agent.enabled;
    if first {
        into.accept_min_keywords = agent.accept_min_keywords;
    }
    for keyword in &agent.keywords {
        let lowered = keyword.trim().to_lowercase();
        if !into
            .keywords
            .iter()
            .any(|known| known.trim().to_lowercase() == lowered)
        {
            into.keywords.push(keyword.clone());
        }
    }
}

fn sorted(values: &[String]) -> Vec<&str> {
    let mut sorted: Vec<&str> = values.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted
}

fn same_criteria(left: &SearchAgent, right: &SearchAgent) -> bool {
    left.enabled == right.enabled
        && left.accept_min_keywords == right.accept_min_keywords
        && left.filters == right.filters
        && sorted(&left.keywords) == sorted(&right.keywords)
        && sorted(&left.exclusions) == sorted(&right.exclusions)
}

fn category_of(provider: &str) -> Option<ProviderCategory> {
    descriptor(provider).map(|descriptor| descriptor.category)
}

fn category_name(providers: &[String]) -> Option<&'static str> {
    let all = |category: ProviderCategory| {
        providers
            .iter()
            .all(|provider| category_of(provider) == Some(category))
    };
    if all(ProviderCategory::JobsAndMissions) {
        Some(JOBS_AND_MISSIONS_NAME)
    } else if all(ProviderCategory::PublicProcurement) {
        Some(PUBLIC_PROCUREMENT_NAME)
    } else {
        None
    }
}

pub(super) fn slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars().flat_map(char::to_lowercase) {
        let folded = match character {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' | 'ì' => 'i',
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ÿ' | 'ý' => 'y',
            'ñ' => 'n',
            'œ' => 'o',
            'æ' => 'a',
            other => other,
        };
        if folded.is_ascii_lowercase() || folded.is_ascii_digit() {
            slug.push(folded);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let bounded: String = slug.chars().take(63).collect();
    bounded.trim_end_matches('-').to_owned()
}

fn unique_id(base: &str, fallback: &AgentId, taken: &[String]) -> AgentId {
    let base = if base.is_empty() {
        fallback.as_str()
    } else {
        base
    };
    if !taken.iter().any(|id| id == base) {
        return AgentId::new(base);
    }
    let mut index = 2u32;
    loop {
        let suffix = format!("-{index}");
        let stem: String = base.chars().take(63 - suffix.len()).collect();
        let candidate = format!("{stem}{suffix}");
        if !taken.contains(&candidate) {
            return AgentId::new(candidate);
        }
        index += 1;
    }
}

fn merge_search_agents(
    agents: Vec<(usize, SearchAgent)>,
    other_ids: &[String],
) -> Vec<(usize, SearchAgent)> {
    let mut groups: Vec<(usize, Vec<SearchAgent>)> = Vec::new();
    for (position, agent) in agents {
        match groups.iter_mut().find(|(_, group)| {
            group
                .first()
                .is_some_and(|first| same_criteria(first, &agent))
        }) {
            Some((_, group)) => group.push(agent),
            None => groups.push((position, vec![agent])),
        }
    }
    let mut taken: Vec<String> = other_ids.to_vec();
    for (_, group) in groups.iter().filter(|(_, group)| group.len() == 1) {
        taken.extend(group.iter().map(|agent| agent.id.as_str().to_owned()));
    }
    let mut merged = Vec::with_capacity(groups.len());
    for (position, mut group) in groups {
        if group.len() == 1 {
            merged.extend(group.into_iter().map(|agent| (position, agent)));
            continue;
        }
        let mut providers: Vec<String> = Vec::new();
        for agent in &mut group {
            for provider in agent.providers.drain(..) {
                if !providers.contains(&provider) {
                    providers.push(provider);
                }
            }
        }
        let first = group.swap_remove(0);
        let name = category_name(&providers).map_or(first.name, str::to_owned);
        let id = unique_id(&slug(&name), &first.id, &taken);
        taken.push(id.as_str().to_owned());
        merged.push((
            position,
            SearchAgent {
                id,
                name,
                providers,
                ..first
            },
        ));
    }
    merged
}

impl ProfileDefinitionV16 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        let mut enrichment = Enrichment::default();
        let mut jobs_seen = false;
        let mut kept: Vec<Agent> = Vec::new();
        let mut searches: Vec<(usize, SearchAgent)> = Vec::new();
        for agent in self.agents {
            match agent {
                AgentV16::Signal(signal) => match signal.source {
                    SignalSourceV16::CertificateTransparency(rules) => {
                        kept.push(Agent::Certificates(CertificatesAgent {
                            id: signal.id,
                            name: signal.name,
                            enabled: signal.enabled,
                            labels: rules.labels,
                        }));
                    }
                    SignalSourceV16::DnsCname(rules) => {
                        kept.push(Agent::Dns(DnsAgent {
                            id: signal.id,
                            name: signal.name,
                            enabled: signal.enabled,
                            patterns: rules.patterns,
                        }));
                    }
                    SignalSourceV16::HttpProbe(rules) => {
                        merge_probe(&mut enrichment.http_probe, rules, signal.enabled);
                    }
                },
                AgentV16::Search(mut search) => {
                    if search.searches(ATS) {
                        merge_company_jobs(&mut enrichment.company_jobs, &search, !jobs_seen);
                        jobs_seen = true;
                        search.providers.retain(|provider| provider != ATS);
                    }
                    if !search.providers.is_empty() {
                        searches.push((kept.len(), search));
                    }
                }
            }
        }
        let other_ids: Vec<String> = kept
            .iter()
            .map(|agent| agent.id().as_str().to_owned())
            .chain(
                self.qualification_agents
                    .iter()
                    .map(|agent| agent.id.as_str().to_owned()),
            )
            .chain(
                self.scoring_agents
                    .iter()
                    .map(|agent| agent.id.as_str().to_owned()),
            )
            .collect();
        let mut ordered: Vec<((usize, u8), Agent)> = kept
            .into_iter()
            .enumerate()
            .map(|(index, agent)| ((index, 1), agent))
            .chain(
                merge_search_agents(searches, &other_ids)
                    .into_iter()
                    .map(|(position, agent)| ((position, 0), Agent::Search(agent))),
            )
            .collect();
        ordered.sort_by_key(|(key, _)| *key);
        let agents: Vec<Agent> = ordered.into_iter().map(|(_, agent)| agent).collect();
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            language: self.language,
            agents,
            enrichment,
            qualification_agents: self.qualification_agents,
            scoring_agents: self.scoring_agents,
            aggregator: self.aggregator,
            escalation: self.escalation,
            verdict: self.verdict,
            evidence: self.evidence,
            contact_ranking: self.contact_ranking,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_str, json};

    use super::slug;
    use crate::domain::profile::{ValidatedProfile, fixtures::MINIMAL_PROFILE};

    fn v16(agents: Value) -> Value {
        let mut value = ValidatedProfile::parse(from_str(MINIMAL_PROFILE).unwrap())
            .unwrap()
            .to_json();
        let object = value.as_object_mut().unwrap();
        object.remove("enrichment");
        object.insert("schema_version".to_owned(), json!(16));
        object.insert("agents".to_owned(), agents);
        value
    }

    fn upgraded(agents: Value) -> Value {
        ValidatedProfile::parse(v16(agents)).unwrap().to_json()
    }

    fn search(id: &str, name: &str, providers: Value, keywords: Value) -> Value {
        json!({"type": "search", "id": id, "name": name, "providers": providers, "keywords": keywords})
    }

    fn probe(id: &str, fingerprints: &[&str], excluded: &[&str], enabled: bool) -> Value {
        let fingerprints: Vec<Value> = fingerprints
            .iter()
            .map(|fingerprint| {
                json!({
                    "id": fingerprint,
                    "path": format!("/{fingerprint}"),
                    "product": fingerprint,
                    "matchers": [{"kind": "status", "equals": 200}]
                })
            })
            .collect();
        json!({
            "type": "signal",
            "id": id,
            "name": id,
            "enabled": enabled,
            "source": {"provider": "http_probe", "fingerprints": fingerprints, "excluded_products": excluded}
        })
    }

    fn ids_and_names(json: &Value) -> Vec<(String, String)> {
        json["agents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|agent| {
                (
                    agent["id"].as_str().unwrap().to_owned(),
                    agent["name"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn certificate_and_dns_signal_agents_become_typed_agents() {
        let json = upgraded(json!([
            {"type": "signal", "id": "certificate-transparency", "name": "Certificate transparency",
             "source": {"provider": "certificate_transparency", "labels": ["sso"]}},
            {"type": "signal", "id": "dns-cname", "name": "DNS CNAME", "enabled": false,
             "source": {"provider": "dns_cname", "patterns": [{"provider": "okta", "target_suffix": "okta.com"}]}}
        ]));
        assert_eq!(
            json["agents"],
            json!([
                {"type": "certificates", "id": "certificate-transparency", "name": "Certificate transparency", "enabled": true, "labels": ["sso"]},
                {"type": "dns", "id": "dns-cname", "name": "DNS CNAME", "enabled": false,
                 "patterns": [{"provider": "okta", "target_suffix": "okta.com"}]}
            ])
        );
        assert_eq!(json["schema_version"], 17);
        assert_eq!(json["enrichment"]["http_probe"]["enabled"], false);
        assert_eq!(json["enrichment"]["company_jobs"]["enabled"], false);
    }

    #[test]
    fn http_probe_agents_move_to_the_enrichment_with_merged_fingerprints() {
        let json = upgraded(json!([
            probe("probe-a", &["keycloak", "authentik"], &["ferriskey"], false),
            search("boamp", "BOAMP", json!(["boamp"]), json!(["sso"])),
            probe(
                "probe-b",
                &["keycloak", "zitadel"],
                &["ferriskey", "own"],
                true
            )
        ]));
        let probe = &json["enrichment"]["http_probe"];
        assert_eq!(probe["enabled"], true);
        let ids: Vec<&str> = probe["fingerprints"]
            .as_array()
            .unwrap()
            .iter()
            .map(|fingerprint| fingerprint["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["keycloak", "authentik", "zitadel"]);
        assert_eq!(probe["excluded_products"], json!(["ferriskey", "own"]));
        assert_eq!(
            probe["extra_subdomains"],
            json!(["auth", "sso", "login", "id", "idp", "accounts"])
        );
        assert_eq!(
            ids_and_names(&json),
            [("boamp".to_owned(), "BOAMP".to_owned())]
        );
    }

    #[test]
    fn ats_leaves_the_search_providers_for_the_company_jobs() {
        let mut fr = search(
            "ats-fr",
            "Job offers (French)",
            json!(["ats"]),
            json!(["Keycloak", "IAM"]),
        );
        fr["accept_min_keywords"] = json!(3);
        let json = upgraded(json!([
            fr,
            search(
                "ats-en",
                "Job offers (English)",
                json!(["ats"]),
                json!(["iam", "SSO"])
            ),
            search(
                "mixed",
                "Mixed",
                json!(["france_travail", "ats"]),
                json!(["Okta"])
            )
        ]));
        assert_eq!(
            json["enrichment"]["company_jobs"],
            json!({"enabled": true, "keywords": ["Keycloak", "IAM", "SSO", "Okta"], "accept_min_keywords": 3})
        );
        assert_eq!(json["agents"].as_array().unwrap().len(), 1);
        assert_eq!(json["agents"][0]["id"], "mixed");
        assert_eq!(json["agents"][0]["providers"], json!(["france_travail"]));
    }

    #[test]
    fn disabled_ats_agents_keep_their_keywords_but_leave_company_jobs_disabled() {
        let mut agent = search("ats", "ATS", json!(["ats", "boamp"]), json!(["sso"]));
        agent["enabled"] = json!(false);
        let json = upgraded(json!([agent]));
        assert_eq!(json["enrichment"]["company_jobs"]["enabled"], false);
        assert_eq!(
            json["enrichment"]["company_jobs"]["keywords"],
            json!(["sso"])
        );
        assert_eq!(json["agents"][0]["providers"], json!(["boamp"]));
    }

    #[test]
    fn identical_job_agents_merge_into_offres_et_missions() {
        let json = upgraded(json!([
            {"type": "signal", "id": "ct", "name": "CT", "source": {"provider": "certificate_transparency", "labels": ["sso"]}},
            search("france-travail", "France Travail", json!(["france_travail"]), json!(["IAM", "SSO"])),
            search("hn", "Hacker News", json!(["hacker_news_hiring"]), json!(["SSO", "IAM"])),
            search("ats-fr", "Jobs", json!(["ats", "france_travail"]), json!(["IAM", "SSO"]))
        ]));
        assert_eq!(
            ids_and_names(&json),
            [
                ("ct".to_owned(), "CT".to_owned()),
                (
                    "offres-et-missions".to_owned(),
                    "Offres et missions".to_owned()
                )
            ]
        );
        assert_eq!(
            json["agents"][1]["providers"],
            json!(["france_travail", "hacker_news_hiring"])
        );
        assert_eq!(json["agents"][1]["keywords"], json!(["IAM", "SSO"]));
    }

    #[test]
    fn identical_procurement_agents_merge_into_marches_publics() {
        let json = upgraded(json!([
            search("boamp", "BOAMP", json!(["boamp"]), json!(["SSO"])),
            search("decp", "DECP", json!(["decp"]), json!(["SSO"])),
            search("ted", "TED", json!(["ted", "boamp"]), json!(["SSO"]))
        ]));
        assert_eq!(
            ids_and_names(&json),
            [("marches-publics".to_owned(), "Marchés publics".to_owned())]
        );
        assert_eq!(
            json["agents"][0]["providers"],
            json!(["boamp", "decp", "ted"])
        );
    }

    #[test]
    fn mixed_categories_keep_the_first_name_and_merge_only_identical_criteria() {
        let mut filtered = search(
            "ft-remote",
            "Remote",
            json!(["france_travail"]),
            json!(["SSO"]),
        );
        filtered["filters"] = json!({"remote": "full_remote"});
        let mut excluded = search("ted-x", "TED exclusions", json!(["ted"]), json!(["SSO"]));
        excluded["exclusions"] = json!(["badge"]);
        let json = upgraded(json!([
            search("veille", "Veille SSO", json!(["boamp"]), json!(["SSO"])),
            filtered,
            excluded,
            search(
                "emplois",
                "Emplois",
                json!(["france_travail"]),
                json!(["SSO"])
            )
        ]));
        assert_eq!(
            ids_and_names(&json),
            [
                ("veille-sso".to_owned(), "Veille SSO".to_owned()),
                ("ft-remote".to_owned(), "Remote".to_owned()),
                ("ted-x".to_owned(), "TED exclusions".to_owned())
            ]
        );
        assert_eq!(
            json["agents"][0]["providers"],
            json!(["boamp", "france_travail"])
        );
    }

    #[test]
    fn a_merged_id_never_collides_with_another_agent() {
        let json = upgraded(json!([
            {"type": "signal", "id": "marches-publics", "name": "Certificats",
             "source": {"provider": "certificate_transparency", "labels": ["sso"]}},
            search("boamp", "BOAMP", json!(["boamp"]), json!(["SSO"])),
            search("ted", "TED", json!(["ted"]), json!(["SSO"]))
        ]));
        assert_eq!(
            ids_and_names(&json),
            [
                ("marches-publics".to_owned(), "Certificats".to_owned()),
                ("marches-publics-2".to_owned(), "Marchés publics".to_owned())
            ]
        );
    }

    #[test]
    fn agents_with_different_enabled_flags_are_not_merged() {
        let mut disabled = search("ted", "TED", json!(["ted"]), json!(["SSO"]));
        disabled["enabled"] = json!(false);
        let json = upgraded(json!([
            search("boamp", "BOAMP", json!(["boamp"]), json!(["SSO"])),
            disabled
        ]));
        assert_eq!(json["agents"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn names_become_slugs_without_accents() {
        assert_eq!(slug("Marchés publics"), "marches-publics");
        assert_eq!(slug("Offres et missions"), "offres-et-missions");
        assert_eq!(slug("  Été — 2026 !"), "ete-2026");
        assert_eq!(slug("***"), "");
        assert_eq!(slug(&"a".repeat(80)).len(), 63);
    }
}
