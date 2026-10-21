use std::time::Duration;

use chrono::{DateTime, Utc};
use common::CoreError;
use dns::HickoryResolver;
use fetch::PoliteFetcher;
use lodestone_core::{
    ContactFinder, LodestoneUseCase, Source, SourceName, StepOutcome,
    contacts::ContactRunSettings,
    infrastructure::{
        candidate_host::postgres::PgCandidateHosts, contact_target::postgres::PgContactTargets,
        ct_cursor::postgres::PgCtCursors,
    },
    jobs::SystemClock,
    listeners::ListenerStep,
    sources::SourceRunError,
};
use sources::{
    contact_pages::PublicPageFinder, ct::CtSource, dns_cname::DnsCnameSource,
    http_probe::HttpProbeSource, role_guess::RoleGuessFinder, security_txt::SecurityTxtFinder,
};

pub type CertificateTransparency = CtSource<PoliteFetcher, PgCandidateHosts, PgCtCursors>;
pub type DnsCname = DnsCnameSource<HickoryResolver, PgCandidateHosts>;
pub type HttpProbe = HttpProbeSource<PoliteFetcher, PgCandidateHosts, LodestoneUseCase>;

fn due_before(now: DateTime<Utc>, revisit: Duration) -> Result<DateTime<Utc>, CoreError> {
    chrono::Duration::from_std(revisit)
        .ok()
        .and_then(|revisit| now.checked_sub_signed(revisit))
        .ok_or_else(|| CoreError::Internal("revisit interval out of range".to_owned()))
}

pub struct CtStep {
    pub usecase: LodestoneUseCase,
    pub source: CertificateTransparency,
}

impl ListenerStep for CtStep {
    fn name(&self) -> &str {
        SourceName::CertificateTransparency.as_str()
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        self.usecase
            .has_enabled_agent(SourceName::CertificateTransparency)
            .await
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        let report = self
            .usecase
            .listen_source(&self.source, &SystemClock)
            .await?;
        let pass = self.source.last_pass();
        Ok(StepOutcome {
            processed: pass.tiles_read.max(report.items),
            signals: report.signals,
            backlog: Some(pass.lag_tiles),
        })
    }
}

pub struct HostStep<S: Source> {
    pub usecase: LodestoneUseCase,
    pub source: S,
    pub hosts: PgCandidateHosts,
    pub revisit: Duration,
}

impl<S: Source> ListenerStep for HostStep<S> {
    fn name(&self) -> &str {
        self.source.name().as_str()
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        self.usecase.has_enabled_agent(self.source.name()).await
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        let before = self
            .hosts
            .count_due(self.source.name(), due_before(Utc::now(), self.revisit)?)
            .await?;
        let report = self
            .usecase
            .listen_source(&self.source, &SystemClock)
            .await?;
        let backlog = self
            .hosts
            .count_due(self.source.name(), due_before(Utc::now(), self.revisit)?)
            .await?;
        Ok(StepOutcome {
            processed: report.items.max(before.saturating_sub(backlog)),
            signals: report.signals,
            backlog: Some(backlog),
        })
    }
}

pub struct ContactStep<F: ContactFinder> {
    pub usecase: LodestoneUseCase,
    pub finder: F,
    pub targets: PgContactTargets,
    pub settings: ContactRunSettings,
}

impl<F: ContactFinder> ListenerStep for ContactStep<F> {
    fn name(&self) -> &str {
        self.finder.name().as_str()
    }

    async fn is_active(&self) -> Result<bool, CoreError> {
        Ok(true)
    }

    async fn run_once(&self) -> Result<StepOutcome, SourceRunError> {
        let due = |at| {
            self.targets
                .count_due(self.finder.name(), at, self.finder.requires())
        };
        let before = due(due_before(Utc::now(), self.settings.revisit_interval)?).await?;
        let report = self
            .usecase
            .listen_contact_finder(&self.finder, &self.targets, self.settings, &SystemClock)
            .await?;
        let backlog = due(due_before(Utc::now(), self.settings.revisit_interval)?).await?;
        Ok(StepOutcome {
            processed: report.domains.max(before.saturating_sub(backlog)),
            signals: report.contacts,
            backlog: Some(backlog),
        })
    }
}

pub type PublicPageStep = ContactStep<PublicPageFinder<PoliteFetcher>>;
pub type SecurityTxtStep = ContactStep<SecurityTxtFinder<PoliteFetcher>>;
pub type RoleGuessStep = ContactStep<RoleGuessFinder<HickoryResolver>>;
pub type DnsCnameStep = HostStep<DnsCname>;
pub type HttpProbeStep = HostStep<HttpProbe>;
