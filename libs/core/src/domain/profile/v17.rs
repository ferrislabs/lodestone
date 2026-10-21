use serde::Deserialize;

use super::definition::{
    Agent, Aggregator, ContactRanking, CustomInspection, Enrichment, Escalation, EvidenceSettings,
    Fingerprint, Identity, Inspection, Language, PROFILE_SCHEMA_VERSION, ProfileDefinition,
    QualificationAgent, ScoringAgent, TargetCompanies, VerdictThresholds,
    default_accept_min_keywords,
};

pub(super) const LEGACY_EXTRA_SUBDOMAINS: [&str; 6] =
    ["auth", "sso", "login", "id", "idp", "accounts"];

const IDENTITY_MARKERS: [&str; 9] = [
    "realm", "auth", "sso", "oidc", "saml", "oauth", "openid", "idp", "identity",
];

fn legacy_extra_subdomains() -> Vec<String> {
    LEGACY_EXTRA_SUBDOMAINS
        .iter()
        .map(|label| (*label).to_owned())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HttpProbeEnrichmentV17 {
    #[serde(default)]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) fingerprints: Vec<Fingerprint>,
    #[serde(default)]
    pub(super) excluded_products: Vec<String>,
    #[serde(default = "legacy_extra_subdomains")]
    pub(super) extra_subdomains: Vec<String>,
}

impl Default for HttpProbeEnrichmentV17 {
    fn default() -> Self {
        Self {
            enabled: false,
            fingerprints: Vec::new(),
            excluded_products: Vec::new(),
            extra_subdomains: legacy_extra_subdomains(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CompanyJobsEnrichmentV17 {
    #[serde(default)]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) keywords: Vec<String>,
    #[serde(default = "default_accept_min_keywords")]
    pub(super) accept_min_keywords: u8,
}

impl Default for CompanyJobsEnrichmentV17 {
    fn default() -> Self {
        Self {
            enabled: false,
            keywords: Vec::new(),
            accept_min_keywords: default_accept_min_keywords(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnrichmentV17 {
    #[serde(default)]
    pub(super) http_probe: HttpProbeEnrichmentV17,
    #[serde(default)]
    pub(super) company_jobs: CompanyJobsEnrichmentV17,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileDefinitionV17 {
    #[serde(rename = "schema_version")]
    pub(super) _schema_version: u32,
    pub(super) identity: Identity,
    #[serde(default)]
    pub(super) language: Language,
    pub(super) agents: Vec<Agent>,
    #[serde(default)]
    pub(super) enrichment: EnrichmentV17,
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

fn is_identity_fingerprint(fingerprint: &Fingerprint) -> bool {
    [&fingerprint.id, &fingerprint.product, &fingerprint.path]
        .iter()
        .any(|text| {
            let lowered = text.to_lowercase();
            IDENTITY_MARKERS
                .iter()
                .any(|marker| lowered.contains(marker))
        })
}

fn is_legacy_default(labels: &[String]) -> bool {
    labels.len() == LEGACY_EXTRA_SUBDOMAINS.len()
        && LEGACY_EXTRA_SUBDOMAINS
            .iter()
            .all(|label| labels.iter().any(|known| known == label))
}

pub(super) fn upgrade_enrichment(enrichment: EnrichmentV17) -> Enrichment {
    let probe = enrichment.http_probe;
    let custom = if probe.enabled {
        let identity = probe.fingerprints.iter().any(is_identity_fingerprint);
        let extra_subdomains = if is_legacy_default(&probe.extra_subdomains) && !identity {
            Vec::new()
        } else {
            probe.extra_subdomains
        };
        CustomInspection {
            fingerprints: probe.fingerprints,
            excluded_products: probe.excluded_products,
            extra_subdomains,
        }
    } else {
        CustomInspection::default()
    };
    let jobs = enrichment.company_jobs;
    Enrichment {
        custom,
        highlight_keywords: if jobs.enabled {
            jobs.keywords
        } else {
            Vec::new()
        },
    }
}

impl ProfileDefinitionV17 {
    pub(super) fn upgrade(self) -> ProfileDefinition {
        ProfileDefinition {
            schema_version: PROFILE_SCHEMA_VERSION,
            identity: self.identity,
            target_companies: TargetCompanies::default(),
            language: self.language,
            agents: self.agents,
            inspection: Inspection::default(),
            enrichment: upgrade_enrichment(self.enrichment),
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

    use crate::domain::profile::{ValidatedProfile, fixtures::MINIMAL_PROFILE};

    fn v17(enrichment: Value) -> Value {
        let mut value = ValidatedProfile::parse(from_str(MINIMAL_PROFILE).unwrap())
            .unwrap()
            .to_json();
        let object = value.as_object_mut().unwrap();
        object.remove("inspection");
        object.insert("schema_version".to_owned(), json!(17));
        object.insert("enrichment".to_owned(), enrichment);
        object.insert(
            "agents".to_owned(),
            json!([{"type": "certificates", "id": "ct", "name": "CT", "labels": ["auth"]}]),
        );
        value
    }

    fn fingerprint(id: &str, path: &str, product: &str) -> Value {
        json!({"id": id, "path": path, "product": product, "matchers": [{"kind": "status", "equals": 200}]})
    }

    fn upgraded(enrichment: Value) -> Value {
        ValidatedProfile::parse(v17(enrichment)).unwrap().to_json()
    }

    #[test]
    fn the_old_iam_default_subdomains_are_dropped_without_identity_fingerprints() {
        let json = upgraded(json!({
            "http_probe": {
                "enabled": true,
                "fingerprints": [fingerprint("magento-version", "/magento_version", "magento")],
                "extra_subdomains": ["auth", "sso", "login", "id", "idp", "accounts"]
            }
        }));
        assert_eq!(json["schema_version"], 19);
        assert_eq!(json["inspection"]["enabled"], true);
        assert_eq!(json["enrichment"]["custom"]["extra_subdomains"], json!([]));
        assert_eq!(
            json["enrichment"]["custom"]["fingerprints"][0]["id"],
            "magento-version"
        );
    }

    #[test]
    fn identity_fingerprints_keep_the_old_default_subdomains() {
        let json = upgraded(json!({
            "http_probe": {
                "enabled": true,
                "fingerprints": [fingerprint("keycloak-realm", "/realms/master", "keycloak")],
                "excluded_products": ["ferriskey"]
            }
        }));
        assert_eq!(
            json["enrichment"]["custom"]["extra_subdomains"],
            json!(["auth", "sso", "login", "id", "idp", "accounts"])
        );
        assert_eq!(
            json["enrichment"]["custom"]["excluded_products"],
            json!(["ferriskey"])
        );
    }

    #[test]
    fn a_custom_subdomain_list_is_kept() {
        let json = upgraded(json!({
            "http_probe": {
                "enabled": true,
                "fingerprints": [fingerprint("magento-version", "/magento_version", "magento")],
                "extra_subdomains": ["www", "shop"]
            }
        }));
        assert_eq!(
            json["enrichment"]["custom"]["extra_subdomains"],
            json!(["www", "shop"])
        );
    }

    #[test]
    fn a_disabled_probe_leaves_no_custom_fingerprint() {
        let json = upgraded(json!({
            "http_probe": {
                "enabled": false,
                "fingerprints": [fingerprint("keycloak-realm", "/realms/master", "keycloak")]
            }
        }));
        assert_eq!(json["enrichment"]["custom"]["fingerprints"], json!([]));
        assert_eq!(json["enrichment"]["custom"]["extra_subdomains"], json!([]));
    }

    #[test]
    fn company_jobs_keywords_become_highlighted_keywords() {
        let enabled = upgraded(json!({
            "company_jobs": {"enabled": true, "keywords": ["Keycloak", "SSO"], "accept_min_keywords": 1}
        }));
        assert_eq!(
            enabled["enrichment"]["highlight_keywords"],
            json!(["Keycloak", "SSO"])
        );
        let disabled = upgraded(json!({
            "company_jobs": {"enabled": false, "keywords": ["Keycloak"]}
        }));
        assert_eq!(disabled["enrichment"]["highlight_keywords"], json!([]));
    }
}
