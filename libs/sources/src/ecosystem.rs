use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use lodestone_core::{
    CompanyBuilder, CompanyPage, DnsRecords, DnsResolver, EcosystemFinding, EcosystemModule,
    Fetcher, Inspection, InspectionTarget, ModuleRunner, OFFICIAL_SOURCE, OfficialData,
    SIREN_FINDING_KEY, SIREN_UNKNOWN, Siren, SourceError,
};
use serde_json::json;

use crate::{
    ats::AtsScanner,
    bodacc::{BodaccSettings, LegalLifeModule},
    company::{CompanyResearch, CompanySettings, find_siren},
    decp::holders::{HolderSettings, PublicContractsModule},
};

pub mod dns_email;
#[cfg(test)]
pub(crate) mod fake;
pub mod hiring;
pub mod recent;
pub(crate) mod registry;
pub mod tech;
pub mod website;

pub use self::{
    dns_email::DnsEmailModule,
    hiring::{HiringModule, MAX_JOB_OFFERS},
    recent::RecentModule,
    website::{WebsiteModule, WebsiteSettings},
};

fn siren_finding(siren: &Siren, source: &str, observed_at: DateTime<Utc>) -> EcosystemFinding {
    EcosystemFinding::new(
        EcosystemModule::OfficialIdentity,
        "siren",
        "SIREN",
        siren.as_str(),
        source,
        observed_at,
    )
    .keyed(SIREN_FINDING_KEY)
    .with_details(json!({
        "public_page": format!("https://annuaire-entreprises.data.gouv.fr/entreprise/{siren}"),
    }))
}

pub fn siren_page<'a>(pages: &'a [CompanyPage], siren: &Siren) -> Option<&'a str> {
    pages
        .iter()
        .skip(1)
        .chain(pages.first())
        .find(|page| find_siren(&page.text).as_ref() == Some(siren))
        .map(|page| page.url.as_str())
}

pub fn identity_inspection(
    siren: Option<&Siren>,
    official: Option<&OfficialData>,
    pages: &[CompanyPage],
    observed_at: DateTime<Utc>,
) -> Inspection {
    let Some(siren) = siren.or(official.map(|official| &official.siren)) else {
        return Inspection::NotApplicable(SIREN_UNKNOWN);
    };
    let register = format!("{OFFICIAL_SOURCE}?q={siren}");
    let siren_source = siren_page(pages, siren).unwrap_or(register.as_str());
    Inspection::Found(match official {
        Some(official) => official_findings(official, siren_source, observed_at),
        None => vec![siren_finding(siren, siren_source, observed_at)],
    })
}

pub fn official_findings(
    official: &OfficialData,
    siren_source: &str,
    observed_at: DateTime<Utc>,
) -> Vec<EcosystemFinding> {
    let module = EcosystemModule::OfficialIdentity;
    let source = official.source_url();
    let mut findings = vec![
        EcosystemFinding::new(
            module,
            "legal_name",
            "Raison sociale",
            official.legal_name.as_str(),
            source.as_str(),
            observed_at,
        )
        .keyed("legal_name"),
        siren_finding(&official.siren, siren_source, observed_at),
    ];
    let activity = match (&official.naf_code, &official.naf_label) {
        (Some(code), Some(label)) => Some(format!("{code}, {label}")),
        (Some(code), None) => Some(code.clone()),
        _ => None,
    };
    if let Some(activity) = activity {
        findings.push(
            EcosystemFinding::new(
                module,
                "activity",
                "Activité principale",
                activity,
                source.as_str(),
                observed_at,
            )
            .keyed("activity"),
        );
    }
    if let Some(range) = &official.headcount_range {
        findings.push(
            EcosystemFinding::new(
                module,
                "headcount",
                "Tranche d'effectif",
                range.as_str(),
                source.as_str(),
                observed_at,
            )
            .keyed("headcount"),
        );
    }
    let place: Vec<&str> = [
        official.head_office_postcode.as_deref(),
        official.head_office_city.as_deref(),
        official.head_office_country.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !place.is_empty() {
        findings.push(
            EcosystemFinding::new(
                module,
                "head_office",
                "Siège",
                place.join(" "),
                source.as_str(),
                observed_at,
            )
            .keyed("head_office"),
        );
    }
    if let Some(created) = official.created_on {
        findings.push(
            EcosystemFinding::new(
                module,
                "created_on",
                "Date de création",
                created.to_string(),
                source.as_str(),
                observed_at,
            )
            .keyed("created_on"),
        );
    }
    findings
}

pub struct IdentityModule<F: Fetcher> {
    research: CompanyResearch<F>,
}

impl<F: Fetcher> IdentityModule<F> {
    pub fn new(research: CompanyResearch<F>) -> Self {
        Self { research }
    }

    pub async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        if let Some(dossier) = target.dossier
            && let Some(built_at) = dossier.built_at
        {
            let observed_at = dossier.content_at.unwrap_or(built_at);
            return Ok(identity_inspection(
                dossier.siren.as_ref(),
                dossier.official.as_ref(),
                &dossier.pages,
                observed_at,
            ));
        }
        let built = self.research.build(target.domain).await?;
        match (&built.official, built.failure) {
            (None, Some(failure)) => Err(SourceError::Upstream(failure)),
            (official, _) => Ok(identity_inspection(
                built.siren.as_ref(),
                official.as_ref(),
                &built.pages,
                target.now,
            )),
        }
    }
}

pub struct EcosystemModules<F: Fetcher, R> {
    pub identity: IdentityModule<F>,
    pub legal: LegalLifeModule<F>,
    pub contracts: PublicContractsModule<F>,
    pub website: WebsiteModule<F>,
    pub dns: DnsEmailModule<R>,
    pub hiring: HiringModule<F>,
    pub recent: RecentModule<F>,
}

#[derive(Debug, Clone)]
pub struct EcosystemConfig {
    pub company: CompanySettings,
    pub bodacc: BodaccSettings,
    pub holders: HolderSettings,
    pub website: WebsiteSettings,
    pub max_career_paths: usize,
    pub max_rate_limit_wait: Duration,
}

impl<F: Fetcher, R: DnsRecords + DnsResolver> EcosystemModules<F, R> {
    pub fn new(fetcher: Arc<F>, resolver: Arc<R>, config: EcosystemConfig) -> Self {
        let research = CompanyResearch::new(Arc::clone(&fetcher), config.company);
        Self {
            legal: LegalLifeModule::new(Arc::clone(&fetcher), config.bodacc, research.clone()),
            contracts: PublicContractsModule::new(
                Arc::clone(&fetcher),
                config.holders,
                research.clone(),
            ),
            identity: IdentityModule::new(research),
            website: WebsiteModule::new(Arc::clone(&fetcher), config.website.clone()),
            dns: DnsEmailModule::new(resolver),
            recent: RecentModule::new(Arc::clone(&fetcher), config.website),
            hiring: HiringModule::new(AtsScanner::new(
                fetcher,
                config.max_career_paths,
                config.max_rate_limit_wait,
            )),
        }
    }

    pub fn runners(&self) -> [EcosystemRunner<'_, F, R>; 7] {
        [
            EcosystemRunner::OfficialIdentity(&self.identity),
            EcosystemRunner::LegalLife(&self.legal),
            EcosystemRunner::PublicContracts(&self.contracts),
            EcosystemRunner::WebsiteStack(&self.website),
            EcosystemRunner::DnsEmail(&self.dns),
            EcosystemRunner::HiringActivity(&self.hiring),
            EcosystemRunner::RecentActivity(&self.recent),
        ]
    }
}

pub enum EcosystemRunner<'a, F: Fetcher, R> {
    OfficialIdentity(&'a IdentityModule<F>),
    LegalLife(&'a LegalLifeModule<F>),
    PublicContracts(&'a PublicContractsModule<F>),
    WebsiteStack(&'a WebsiteModule<F>),
    DnsEmail(&'a DnsEmailModule<R>),
    HiringActivity(&'a HiringModule<F>),
    RecentActivity(&'a RecentModule<F>),
}

impl<F: Fetcher, R: DnsRecords + DnsResolver> ModuleRunner for EcosystemRunner<'_, F, R> {
    fn module(&self) -> EcosystemModule {
        match self {
            Self::OfficialIdentity(_) => EcosystemModule::OfficialIdentity,
            Self::LegalLife(_) => EcosystemModule::LegalLife,
            Self::PublicContracts(_) => EcosystemModule::PublicContracts,
            Self::WebsiteStack(_) => EcosystemModule::WebsiteStack,
            Self::DnsEmail(_) => EcosystemModule::DnsEmail,
            Self::HiringActivity(_) => EcosystemModule::HiringActivity,
            Self::RecentActivity(_) => EcosystemModule::RecentActivity,
        }
    }

    async fn inspect(&self, target: &InspectionTarget<'_>) -> Result<Inspection, SourceError> {
        match self {
            Self::OfficialIdentity(module) => module.inspect(target).await,
            Self::LegalLife(module) => module.inspect(target).await,
            Self::PublicContracts(module) => module.inspect(target).await,
            Self::WebsiteStack(module) => module.inspect(target).await.map(Inspection::Found),
            Self::DnsEmail(module) => module.inspect(target).await.map(Inspection::Found),
            Self::HiringActivity(module) => module.inspect(target).await.map(Inspection::Found),
            Self::RecentActivity(module) => module.inspect(target).await.map(Inspection::Found),
        }
    }
}

#[cfg(test)]
mod tests;
