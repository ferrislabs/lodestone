use std::time::Duration;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;
use tracing::{debug, info};

use crate::{
    application::{
        LodestoneUseCase,
        sources::{SourceRunError, rescore},
    },
    domain::{
        NormalizedDomain,
        contact::{ContactFinder, ContactTargets, FoundAddress, select_for_profile},
        job::ports::Clock,
        lead::{
            ContactEmail, ContactId, ContactKind, ContactMethod, Lead, Signal, SignalId,
            ports::{ContactRepository, LeadRepository, SignalRepository},
        },
        profile::ActiveProfile,
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::SourceError,
        suppression::ports::SuppressionRepository,
        values::{SignalKind, SourceName},
    },
};

pub const CONTACT_SIGNAL: &str = "role_contact_found";
pub const SECURITY_TXT_SIGNAL: &str = "security_txt_contact";
pub const GUESSED_CONTACT_SIGNAL: &str = "role_contact_guessed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactRunSettings {
    pub batch_size: u32,
    pub revisit_interval: Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContactRunReport {
    pub domains: u64,
    pub contacts: u64,
    pub rate_limited: bool,
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

impl LodestoneUseCase {
    pub async fn run_contact_finder<F, T, C>(
        &self,
        finder: &F,
        targets: &T,
        settings: ContactRunSettings,
        clock: &C,
    ) -> Result<ContactRunReport, SourceRunError>
    where
        F: ContactFinder,
        T: ContactTargets,
        C: Clock,
    {
        let name = finder.name();
        let profiles = self.list_active_profiles().await?;
        if profiles.is_empty() {
            return Ok(ContactRunReport::default());
        }
        let now = clock.now();
        let revisit = chrono::Duration::from_std(settings.revisit_interval)
            .map_err(|_| CoreError::Internal("revisit interval out of range".to_owned()))?;
        let domains = targets
            .list_due(name, now - revisit, settings.batch_size, finder.requires())
            .await?;

        let runs: Vec<Run> = profiles
            .iter()
            .map(|active| Run::start(RunId(generate_uuid_v7()), active.profile.id, name, now))
            .collect();
        for run in &runs {
            self.start_contact_run(run).await?;
        }

        let mut report = ContactRunReport::default();
        let mut checked = Vec::with_capacity(domains.len());
        let mut per_profile = vec![0u64; profiles.len()];
        for domain in &domains {
            let found = match finder.find(domain, &profiles, clock.now()).await {
                Ok(found) => found,
                Err(SourceError::RateLimited { .. }) => {
                    report.rate_limited = true;
                    break;
                }
                Err(error) => {
                    debug!(%domain, %error, "contact finder skipped a domain");
                    continue;
                }
            };
            targets
                .mark_checked(std::slice::from_ref(domain), name, clock.now())
                .await?;
            checked.push(domain.clone());
            if found.is_empty() {
                continue;
            }
            for (index, active) in profiles.iter().enumerate() {
                let stored = self
                    .record_contacts(active, domain, &found, name, clock.now())
                    .await?;
                per_profile[index] += stored;
                report.contacts += stored;
            }
        }
        report.domains = count(checked.len());

        for (run, contacts) in runs.iter().zip(per_profile) {
            let outcome = RunOutcome::Succeeded {
                volume: report.domains,
                signals: contacts,
            };
            let finished_at = clock.now();
            self.finish_contact_run(run.id, &outcome, finished_at)
                .await?;
            self.observe_run(run, &outcome, finished_at);
        }
        info!(source = %name, domains = report.domains, contacts = report.contacts, rate_limited = report.rate_limited, "contact run finished");
        Ok(report)
    }

    #[transactional(run)]
    async fn start_contact_run(&self, run: &Run) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.insert(run).await
    }

    #[transactional(run)]
    async fn finish_contact_run(
        &self,
        id: RunId,
        outcome: &RunOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.finish(id, outcome, at).await?;
        Ok(())
    }

    #[transactional(lead, contact, signal, suppression)]
    pub(crate) async fn record_contacts(
        &self,
        active: &ActiveProfile,
        domain: &NormalizedDomain,
        found: &[FoundAddress],
        source: SourceName,
        now: DateTime<Utc>,
    ) -> Result<u64, CoreError> {
        let mut leads = lead_repository;
        let mut contacts = contact_repository;
        let mut signals = signal_repository;
        let mut suppressions = suppression_repository;
        if suppressions.is_domain_suppressed(domain).await? {
            return Ok(0);
        }
        let Some(lead) = leads
            .list_by_domain(domain)
            .await?
            .into_iter()
            .find(|lead: &Lead| lead.profile_id == active.profile.id)
        else {
            return Ok(0);
        };
        let ranking = &active.revision.definition.definition().contact_ranking;
        let existing = contacts.list_for_lead(lead.id).await?;
        let has_real_address = existing.iter().any(|contact| {
            matches!(
                contact.method,
                ContactMethod::PublicPage | ContactMethod::LlmExtraction
            )
        });
        let known: Vec<String> = existing
            .into_iter()
            .map(|contact| contact.address.to_string())
            .collect();
        let mut stored = 0;
        for candidate in select_for_profile(found, domain, ranking) {
            if known.contains(&candidate.address.to_string())
                || (candidate.method == ContactMethod::RoleGuess && has_real_address)
                || suppressions.is_email_suppressed(&candidate.address).await?
            {
                continue;
            }
            contacts
                .insert(&ContactEmail {
                    id: ContactId(generate_uuid_v7()),
                    lead_id: lead.id,
                    address: candidate.address.clone(),
                    provenance: candidate.provenance.clone(),
                    method: candidate.method,
                    confidence: candidate.confidence,
                    kind: ContactKind::Role,
                    found_at: now,
                })
                .await?;
            let kind = SignalKind::parse(match candidate.method {
                ContactMethod::SecurityTxt => SECURITY_TXT_SIGNAL,
                ContactMethod::RoleGuess => GUESSED_CONTACT_SIGNAL,
                ContactMethod::PublicPage | ContactMethod::LlmExtraction => CONTACT_SIGNAL,
            })
            .map_err(|_| CoreError::Internal("invalid contact signal kind".to_owned()))?;
            signals
                .record(&Signal {
                    id: SignalId(generate_uuid_v7()),
                    lead_id: lead.id,
                    kind,
                    value: Some(candidate.address.local_part().to_owned()),
                    source,
                    raw: json!({ "address": candidate.address.as_str(), "method": candidate.method.as_str() }),
                    provenance: candidate.provenance.clone(),
                    strength: candidate.confidence,
                    observed_at: now,
                })
                .await?;
            stored += 1;
        }
        if stored > 0 {
            rescore(
                &mut leads,
                &mut signals,
                lead.id,
                &active.revision.definition.definition().scoring,
                now,
            )
            .await?;
        }
        Ok(stored)
    }
}
