use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;

use crate::domain::{
    NormalizedDomain,
    company::{CompanyDossier, DossierDocument},
    lead::str_enum,
    profile::{
        ProfileId,
        definition::{Fingerprint, ProfileDefinition},
    },
    source::SourceError,
};

mod platform;
pub mod ports;

pub use platform::{HOSTING_PLATFORMS, hosting_platform};

pub const MAX_DOCUMENT_FINDINGS: usize = 12;
pub const STATS_WINDOW_DAYS: i64 = 7;
pub const STATS_RETENTION_DAYS: i64 = 30;
const FAILURE_RETRY_HOURS: i64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EcosystemModule {
    OfficialIdentity,
    LegalLife,
    PublicContracts,
    WebsiteStack,
    DnsEmail,
    HiringActivity,
    RecentActivity,
}

str_enum!(EcosystemModule {
    OfficialIdentity => "official_identity",
    LegalLife => "legal_life",
    PublicContracts => "public_contracts",
    WebsiteStack => "website_stack",
    DnsEmail => "dns_email",
    HiringActivity => "hiring_activity",
    RecentActivity => "recent_activity",
});

impl EcosystemModule {
    pub const ALL: [Self; 7] = [
        Self::OfficialIdentity,
        Self::LegalLife,
        Self::PublicContracts,
        Self::WebsiteStack,
        Self::DnsEmail,
        Self::HiringActivity,
        Self::RecentActivity,
    ];

    pub const AVAILABLE: [Self; 3] = [
        Self::OfficialIdentity,
        Self::WebsiteStack,
        Self::HiringActivity,
    ];

    pub fn is_available(self) -> bool {
        Self::AVAILABLE.contains(&self)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OfficialIdentity => "Identité officielle",
            Self::LegalLife => "Vie légale",
            Self::PublicContracts => "Marchés publics",
            Self::WebsiteStack => "Site et stack technique",
            Self::DnsEmail => "Écosystème DNS et e-mail",
            Self::HiringActivity => "Activité de recrutement",
            Self::RecentActivity => "Activité récente",
        }
    }

    pub fn question(self) -> &'static str {
        match self {
            Self::OfficialIdentity => "Qui est officiellement cette entreprise ?",
            Self::LegalLife => "Que dit sa vie légale ?",
            Self::PublicContracts => "Remporte-t-elle des marchés publics ?",
            Self::WebsiteStack => "Sur quoi tourne son site ?",
            Self::DnsEmail => "Quels services utilise-t-elle pour son domaine et ses e-mails ?",
            Self::HiringActivity => "Recrute-t-elle, et sur quels postes ?",
            Self::RecentActivity => "Qu'a-t-elle publié récemment ?",
        }
    }

    pub fn learns(self) -> &'static str {
        match self {
            Self::OfficialIdentity => {
                "Raison sociale, SIREN, activité principale, tranche d'effectif, siège et date de création, d'après le registre officiel des entreprises."
            }
            Self::LegalLife => {
                "Événements légaux publiés au BODACC : créations, modifications, procédures collectives, cessions."
            }
            Self::PublicContracts => {
                "Marchés publics dont l'entreprise est titulaire, d'après les données essentielles de la commande publique (DECP)."
            }
            Self::WebsiteStack => {
                "Serveur, technologies et produits visibles sur le site et ses sous-domaines, avec les empreintes propres au profil."
            }
            Self::DnsEmail => {
                "Fournisseurs de messagerie, d'hébergement et de services déclarés dans les enregistrements DNS du domaine."
            }
            Self::HiringActivity => {
                "Page carrières, outil de recrutement et offres d'emploi publiées par l'entreprise."
            }
            Self::RecentActivity => {
                "Actualités, articles et annonces publiés récemment sur le site de l'entreprise."
            }
        }
    }

    pub fn english_name(self) -> &'static str {
        match self {
            Self::OfficialIdentity => "official identity",
            Self::LegalLife => "legal life",
            Self::PublicContracts => "public contracts",
            Self::WebsiteStack => "website and technical stack",
            Self::DnsEmail => "DNS and e-mail ecosystem",
            Self::HiringActivity => "hiring activity",
            Self::RecentActivity => "recent activity",
        }
    }

    pub fn reads_site(self) -> bool {
        !matches!(self, Self::LegalLife | Self::PublicContracts)
    }

    pub fn refresh_interval(self) -> Duration {
        match self {
            Self::OfficialIdentity => Duration::days(30),
            Self::LegalLife | Self::PublicContracts | Self::WebsiteStack | Self::DnsEmail => {
                Duration::days(7)
            }
            Self::HiringActivity | Self::RecentActivity => Duration::days(3),
        }
    }

    pub fn failure_retry(self) -> Duration {
        Duration::hours(FAILURE_RETRY_HOURS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModuleStatus {
    Pending,
    Done,
    NotApplicable,
    Failed,
}

str_enum!(ModuleStatus {
    Pending => "pending",
    Done => "done",
    NotApplicable => "not_applicable",
    Failed => "failed",
});

impl ModuleStatus {
    pub fn is_settled(self) -> bool {
        self != Self::Pending
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleState {
    pub module: EcosystemModule,
    pub status: ModuleStatus,
    pub last_run: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl ModuleState {
    pub fn pending(module: EcosystemModule) -> Self {
        Self {
            module,
            status: ModuleStatus::Pending,
            last_run: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EcosystemFinding {
    pub module: EcosystemModule,
    pub key: String,
    pub kind: String,
    pub label: String,
    pub value: String,
    pub source_url: String,
    pub observed_at: DateTime<Utc>,
    pub details: Value,
}

impl Serialize for EcosystemModule {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EcosystemModule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| D::Error::custom("unknown ecosystem module"))
    }
}

impl EcosystemFinding {
    pub fn new(
        module: EcosystemModule,
        kind: &str,
        label: &str,
        value: impl Into<String>,
        source_url: impl Into<String>,
        observed_at: DateTime<Utc>,
    ) -> Self {
        let value = value.into();
        Self {
            module,
            key: format!("{kind}:{value}"),
            kind: kind.to_owned(),
            label: label.to_owned(),
            value,
            source_url: source_url.into(),
            observed_at,
            details: Value::Null,
        }
    }

    pub fn keyed(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn example(&self, domain: &NormalizedDomain) -> FindingExample {
        FindingExample {
            domain: domain.as_str().to_owned(),
            label: self.label.clone(),
            value: self.value.clone(),
            source_url: self.source_url.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindingExample {
    pub domain: String,
    pub label: String,
    pub value: String,
    pub source_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleOutcome {
    Done(Vec<EcosystemFinding>),
    NotApplicable,
    Failed(String),
}

impl ModuleOutcome {
    pub fn status(&self) -> ModuleStatus {
        match self {
            Self::Done(_) => ModuleStatus::Done,
            Self::NotApplicable => ModuleStatus::NotApplicable,
            Self::Failed(_) => ModuleStatus::Failed,
        }
    }

    pub fn findings(&self) -> &[EcosystemFinding] {
        match self {
            Self::Done(findings) => findings,
            Self::NotApplicable | Self::Failed(_) => &[],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InspectionSettings {
    pub fingerprints: Vec<Fingerprint>,
    pub excluded_products: Vec<String>,
    pub extra_subdomains: Vec<String>,
    pub highlight_keywords: Vec<String>,
}

impl InspectionSettings {
    pub fn merge(&mut self, definition: &ProfileDefinition) {
        let custom = &definition.enrichment.custom;
        for fingerprint in &custom.fingerprints {
            if !self
                .fingerprints
                .iter()
                .any(|known| known.id == fingerprint.id)
            {
                self.fingerprints.push(fingerprint.clone());
            }
        }
        for product in &custom.excluded_products {
            if !self.excluded_products.contains(product) {
                self.excluded_products.push(product.clone());
            }
        }
        for label in &custom.extra_subdomains {
            if !self.extra_subdomains.contains(label) {
                self.extra_subdomains.push(label.clone());
            }
        }
        for keyword in definition.company_jobs_keywords() {
            let lowered = keyword.to_lowercase();
            if !self
                .highlight_keywords
                .iter()
                .any(|known| known.to_lowercase() == lowered)
            {
                self.highlight_keywords.push(keyword);
            }
        }
    }

    pub fn of<'a>(definitions: impl IntoIterator<Item = &'a ProfileDefinition>) -> Self {
        let mut settings = Self::default();
        for definition in definitions {
            settings.merge(definition);
        }
        settings
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InspectionTarget<'a> {
    pub domain: &'a NormalizedDomain,
    pub settings: &'a InspectionSettings,
    pub dossier: Option<&'a CompanyDossier>,
    pub now: DateTime<Utc>,
}

pub trait ModuleRunner: Send + Sync {
    fn module(&self) -> EcosystemModule;

    fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> impl Future<Output = Result<Vec<EcosystemFinding>, SourceError>> + Send;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueDomain {
    pub domain: NormalizedDomain,
    pub leads: u64,
    pub profiles: Vec<ProfileId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRun {
    pub module: EcosystemModule,
    pub ran_at: DateTime<Utc>,
    pub leads_inspected: u64,
    pub findings_count: u64,
    pub latest_example: Option<FindingExample>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleStats {
    pub module: EcosystemModule,
    pub leads_inspected: u64,
    pub findings_count: u64,
    pub last_run: Option<DateTime<Utc>>,
    pub latest_example: Option<FindingExample>,
}

impl ModuleStats {
    pub fn empty(module: EcosystemModule) -> Self {
        Self {
            module,
            leads_inspected: 0,
            findings_count: 0,
            last_run: None,
            latest_example: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DomainEcosystem {
    pub modules: Vec<ModuleState>,
    pub findings: Vec<EcosystemFinding>,
}

impl DomainEcosystem {
    pub fn state_of(&self, module: EcosystemModule) -> ModuleState {
        self.modules
            .iter()
            .find(|state| state.module == module)
            .cloned()
            .unwrap_or_else(|| ModuleState::pending(module))
    }

    pub fn findings_of(&self, module: EcosystemModule) -> impl Iterator<Item = &EcosystemFinding> {
        self.findings
            .iter()
            .filter(move |finding| finding.module == module)
    }
}

pub fn ecosystem_documents(findings: &[EcosystemFinding]) -> Vec<DossierDocument> {
    let mut groups: Vec<(EcosystemModule, &str, Vec<&EcosystemFinding>)> = Vec::new();
    for module in EcosystemModule::ALL {
        for finding in findings
            .iter()
            .filter(|finding| finding.module == module)
            .take(MAX_DOCUMENT_FINDINGS)
        {
            match groups.iter_mut().find(|(known, source, _)| {
                *known == module && *source == finding.source_url.as_str()
            }) {
                Some((_, _, members)) => members.push(finding),
                None => groups.push((module, finding.source_url.as_str(), vec![finding])),
            }
        }
    }
    groups
        .into_iter()
        .map(|(module, source, members)| {
            let mut lines = vec![format!(
                "Ecosystem, module {} ({}), observed by the system at {source}:",
                module.english_name(),
                module.as_str()
            )];
            lines.extend(
                members
                    .iter()
                    .map(|finding| format!("- {}: {}", finding.label, finding.value)),
            );
            DossierDocument {
                source: source.to_owned(),
                content: lines.join("\n"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
