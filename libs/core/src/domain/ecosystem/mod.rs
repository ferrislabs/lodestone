use std::collections::BTreeMap;

use aws_lc_rs::digest::{SHA256, digest};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::{Value, to_string};

use crate::domain::{
    NormalizedDomain,
    company::{CompanyDossier, DossierDocument, DossierState, Siren},
    lead::str_enum,
    profile::{
        ProfileId,
        definition::{Fingerprint, ProfileDefinition},
    },
    qualification::prompt::hex,
    source::SourceError,
    source_provider::{BODACC, COMPANY_RESEARCH, DECP},
};

mod platform;
pub mod ports;

pub use platform::{
    HOSTING_PLATFORMS, SAAS_TENANT_DOMAINS, SAAS_TENANT_REASON, hosting_platform, saas_tenant,
};

pub const MAX_DOCUMENT_FINDINGS: usize = 12;
pub const MAX_WEBSITE_DOCUMENT_FINDINGS: usize = 24;
pub const STATS_WINDOW_DAYS: i64 = 7;
pub const STATS_RETENTION_DAYS: i64 = 30;
pub const SIREN_UNKNOWN: &str = "SIREN inconnu";
pub const SIREN_FINDING_KEY: &str = "siren";
pub const PROVIDER_NOT_READY: &str = "Fournisseur non prêt";
pub const RECENT_LEAD_HOURS: i64 = 1;
const FAILURE_RETRY_HOURS: i64 = 6;
const PASS_RELATIVE_KINDS: [&str; 2] = ["freshness", "hiring_summary"];
const PASS_RELATIVE_DETAILS: [&str; 4] = ["days", "previous_count", "delta", "history"];

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

    pub const AVAILABLE: [Self; 7] = Self::ALL;

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
                "CMS, frameworks, hébergement et CDN, mesure d'audience, chat, outils marketing, e-commerce, authentification et paiement visibles sur le site, avec les empreintes propres au profil."
            }
            Self::DnsEmail => {
                "Fournisseur de messagerie (MX), services d'envoi (SPF), outils vérifiés (TXT), politique DMARC et services SaaS des sous-domaines courants."
            }
            Self::HiringActivity => {
                "Toutes les offres publiées (page carrières et outils de recrutement) : nombre, métiers, lieux, télétravail, contrats et évolution depuis le passage précédent."
            }
            Self::RecentActivity => {
                "Pages mises à jour ces 90 derniers jours (sitemap), dernières actualités du blog et fraîcheur du site."
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
        !matches!(
            self,
            Self::LegalLife | Self::PublicContracts | Self::DnsEmail
        )
    }

    pub fn keeps_history(self) -> bool {
        self == Self::HiringActivity
    }

    pub fn document_limit(self) -> usize {
        match self {
            Self::WebsiteStack => MAX_WEBSITE_DOCUMENT_FINDINGS,
            _ => MAX_DOCUMENT_FINDINGS,
        }
    }

    pub fn provider(self) -> Option<&'static str> {
        match self {
            Self::OfficialIdentity => Some(COMPANY_RESEARCH),
            Self::LegalLife => Some(BODACC),
            Self::PublicContracts => Some(DECP),
            Self::WebsiteStack | Self::DnsEmail | Self::HiringActivity | Self::RecentActivity => {
                None
            }
        }
    }

    pub fn needs_siren(self) -> bool {
        matches!(
            self,
            Self::OfficialIdentity | Self::LegalLife | Self::PublicContracts
        )
    }

    pub fn refresh_interval(self) -> Duration {
        match self {
            Self::OfficialIdentity => Duration::days(30),
            Self::PublicContracts => Duration::days(14),
            Self::LegalLife | Self::WebsiteStack | Self::DnsEmail => Duration::days(7),
            Self::HiringActivity | Self::RecentActivity => Duration::days(3),
        }
    }

    pub fn failure_retry(self) -> Duration {
        Duration::hours(FAILURE_RETRY_HOURS)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EcosystemReadiness {
    not_ready: Vec<EcosystemModule>,
}

impl EcosystemReadiness {
    pub fn with_not_ready(not_ready: Vec<EcosystemModule>) -> Self {
        Self { not_ready }
    }

    pub fn from_ready_providers(ready: &[String]) -> Self {
        Self {
            not_ready: EcosystemModule::AVAILABLE
                .into_iter()
                .filter(|module| {
                    module
                        .provider()
                        .is_some_and(|provider| !ready.iter().any(|key| key == provider))
                })
                .collect(),
        }
    }

    pub fn is_ready(&self, module: EcosystemModule) -> bool {
        !self.not_ready.contains(&module)
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
    pub changed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl ModuleState {
    pub fn pending(module: EcosystemModule) -> Self {
        Self {
            module,
            status: ModuleStatus::Pending,
            last_run: None,
            changed_at: None,
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

    fn detail_text(&self, key: &str) -> Option<&str> {
        self.details
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }

    pub fn document_line(&self) -> String {
        let mut line = match self.detail_text("date") {
            Some(date) => format!("- {} ({date}): {}", self.label, self.value),
            None => format!("- {}: {}", self.label, self.value),
        };
        let mut extras: Vec<String> = Vec::new();
        if let Some(buyer) = self.detail_text("buyer") {
            extras.push(format!("buyer {buyer}"));
        }
        if let Some(amount) = self.details.get("amount_eur").and_then(Value::as_f64) {
            extras.push(format!("amount {} EUR", euros(amount)));
        }
        if let Some(procedure) = self.detail_text("procedure") {
            extras.push(format!("procedure {procedure}"));
        }
        if let Some(court) = self.detail_text("court") {
            extras.push(format!("court {court}"));
        }
        if !extras.is_empty() {
            line.push_str("; ");
            line.push_str(&extras.join("; "));
        }
        line
    }

    fn content(&self) -> String {
        let value = (!PASS_RELATIVE_KINDS.contains(&self.kind.as_str())).then_some(&self.value);
        let details: Option<BTreeMap<&str, &Value>> = self.details.as_object().map(|map| {
            map.iter()
                .filter(|(key, _)| !PASS_RELATIVE_DETAILS.contains(&key.as_str()))
                .map(|(key, value)| (key.as_str(), value))
                .collect()
        });
        let raw = details.is_none().then_some(&self.details);
        to_string(&(&self.key, &self.kind, &self.label, value, details, raw)).unwrap_or_default()
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
    NotApplicable(Option<&'static str>),
    Failed(String),
}

impl ModuleOutcome {
    pub fn status(&self) -> ModuleStatus {
        match self {
            Self::Done(_) => ModuleStatus::Done,
            Self::NotApplicable(_) => ModuleStatus::NotApplicable,
            Self::Failed(_) => ModuleStatus::Failed,
        }
    }

    pub fn findings(&self) -> &[EcosystemFinding] {
        match self {
            Self::Done(findings) => findings,
            Self::NotApplicable(_) | Self::Failed(_) => &[],
        }
    }

    pub fn note(&self) -> Option<&str> {
        match self {
            Self::Failed(message) => Some(message),
            Self::NotApplicable(reason) => *reason,
            Self::Done(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inspection {
    Found(Vec<EcosystemFinding>),
    NotApplicable(&'static str),
}

impl Inspection {
    pub fn outcome(self) -> ModuleOutcome {
        match self {
            Self::Found(findings) => ModuleOutcome::Done(findings),
            Self::NotApplicable(reason) => ModuleOutcome::NotApplicable(Some(reason)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SirenLookup<'a> {
    Known(&'a Siren),
    Unknown,
    Unresolved,
}

impl<'a> SirenLookup<'a> {
    pub fn of(
        dossier: Option<&'a CompanyDossier>,
        recorded: Option<&'a Siren>,
        identity_settled: bool,
    ) -> Self {
        let from_dossier = dossier.and_then(|dossier| {
            dossier
                .siren
                .as_ref()
                .or_else(|| dossier.official.as_ref().map(|official| &official.siren))
        });
        if let Some(siren) = from_dossier.or(recorded) {
            return Self::Known(siren);
        }
        let dossier_built = dossier.is_some_and(|dossier| {
            dossier.built_at.is_some() && dossier.state != DossierState::Pending
        });
        if dossier_built || identity_settled {
            Self::Unknown
        } else {
            Self::Unresolved
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
    pub siren: SirenLookup<'a>,
    pub previous: &'a [EcosystemFinding],
    pub now: DateTime<Utc>,
}

pub trait ModuleRunner: Send + Sync {
    fn module(&self) -> EcosystemModule;

    fn inspect(
        &self,
        target: &InspectionTarget<'_>,
    ) -> impl Future<Output = Result<Inspection, SourceError>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueBatch {
    pub priority: u32,
    pub refresh: u32,
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

    pub fn recorded_siren(&self) -> Option<Siren> {
        self.findings_of(EcosystemModule::OfficialIdentity)
            .find(|finding| finding.key == SIREN_FINDING_KEY)
            .and_then(|finding| Siren::parse(&finding.value).ok())
    }

    pub fn identity_settled(&self) -> bool {
        matches!(
            self.state_of(EcosystemModule::OfficialIdentity).status,
            ModuleStatus::Done | ModuleStatus::NotApplicable
        )
    }
}

fn euros(amount: f64) -> String {
    if amount.fract() == 0.0 {
        format!("{amount:.0}")
    } else {
        format!("{amount:.2}")
    }
}

pub fn findings_hash(findings: &[EcosystemFinding]) -> String {
    let mut contents: Vec<String> = findings.iter().map(EcosystemFinding::content).collect();
    contents.sort();
    hex(digest(&SHA256, contents.join("\n").as_bytes()).as_ref())
}

pub fn ecosystem_documents(findings: &[EcosystemFinding]) -> Vec<DossierDocument> {
    let mut groups: Vec<(EcosystemModule, &str, Vec<&EcosystemFinding>)> = Vec::new();
    for module in EcosystemModule::ALL {
        for finding in findings
            .iter()
            .filter(|finding| finding.module == module)
            .take(module.document_limit())
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
            lines.extend(members.iter().map(|finding| finding.document_line()));
            DossierDocument {
                source: source.to_owned(),
                content: lines.join("\n"),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
