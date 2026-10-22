use std::{num::NonZeroU16, sync::Arc};

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use futures_util::{StreamExt, stream};
use lodestone_macros::transactional;
use serde_json::json;
use tokio::sync::watch;
use tracing::info;

use crate::{
    application::{
        LodestoneUseCase,
        evaluation::{
            Attempt, EvaluationSettings, LeadMaterial, ProviderBuilder, Providers, Qualified,
            TrialWorks, qualify_in_memory,
        },
        jobs::JobError,
        llm::gateway::RunBudget,
        scoring_agent::{Scored, Scoring, score_in_memory},
    },
    domain::{
        agent_work::{ProfileAgents, WorkSnapshot},
        calibration::Calibrations,
        job::{Job, JobKind, NewJob, ports::Clock},
        lead::{Lead, LeadId, ports::LeadRepository},
        llm::{cache::LlmCache, provider::SecretCipher},
        output::ports::JobOutbox,
        profile::{ActiveProfile, Profile, ProfileId, ProfileRevision, ports::ProfileRepository},
        qualification::{ResultStatus, exclusions},
        scoring_agent::decide,
        trial::{
            DecidedLead, NO_DECIDED_LEAD, NO_VALID_ANSWER, NewTrial, ProfileTrial, Reading,
            TRIAL_ALREADY_LIVE, TRIAL_LIST_LIMIT, TrialId, TrialLead, TrialStatus, TrialSummary,
            ports::{ProfileTrialRepository, Transition},
            select_sample,
        },
    },
    infrastructure::job::postgres::PgJobOutbox,
};

pub const PROFILE_TRIAL_JOB_KIND: &str = "profile.trial";
const TRIAL_JOB_MAX_ATTEMPTS: NonZeroU16 = NonZeroU16::new(3).unwrap();
const BASE_REVISION_MISSING: &str = "the base revision of the trial no longer exists";

pub fn trial_job(trial_id: TrialId, run_at: DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(PROFILE_TRIAL_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid trial job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({ "trial_id": trial_id.to_string() }),
        run_at,
        max_attempts: TRIAL_JOB_MAX_ATTEMPTS,
        dedup_key: Some(format!("trial:{trial_id}")),
    })
}

pub fn trial_of_job(job: &Job) -> Result<TrialId, JobError> {
    job.payload
        .get("trial_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| JobError::Permanent("trial job without a trial".to_owned()))?
        .parse()
        .map_err(|_| JobError::Permanent("trial job with an invalid trial".to_owned()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrialReport {
    pub trial: ProfileTrial,
    pub leads: Vec<TrialLead>,
    pub summary: TrialSummary,
}

struct TrialRun<'a, B: ProviderBuilder, S, C, K> {
    services: (&'a B, &'a S, &'a C),
    clock: &'a K,
    settings: EvaluationSettings,
    active: &'a ActiveProfile,
    calibrations: &'a Calibrations,
    providers: &'a Providers<B::Provider>,
    budget: &'a RunBudget,
    works: &'a TrialWorks,
}

enum Tried {
    Lead(Box<TrialLead>),
    BudgetExhausted,
    Missing,
}

impl LodestoneUseCase {
    #[transactional(profile, profile_trial)]
    pub async fn create_profile_trial(
        &self,
        new: NewTrial,
        now: DateTime<Utc>,
    ) -> Result<ProfileTrial, CoreError> {
        let mut profiles = profile_repository;
        let mut trials = profile_trial_repository;
        profiles
            .find(new.profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let base = profiles
            .list_revisions(new.profile_id)
            .await?
            .into_iter()
            .map(|revision| revision.number)
            .max()
            .ok_or(CoreError::NotFound)?;
        let decided = trials.decided_leads(new.profile_id).await?;
        if select_sample(&decided, new.sample_size).is_empty() {
            return Err(CoreError::Conflict(NO_DECIDED_LEAD.to_owned()));
        }
        let trial = ProfileTrial::pending(TrialId(generate_uuid_v7()), base, new, now);
        trials.create(&trial).await.map_err(|error| match error {
            CoreError::Conflict(_) => CoreError::Conflict(TRIAL_ALREADY_LIVE.to_owned()),
            other => other,
        })?;
        PgJobOutbox::new(&tx)
            .enqueue(&trial_job(trial.id, now)?, now)
            .await?;
        Ok(trial)
    }

    #[transactional(profile, profile_trial)]
    pub async fn list_profile_trials(
        &self,
        profile_id: ProfileId,
    ) -> Result<Vec<ProfileTrial>, CoreError> {
        let mut profiles = profile_repository;
        let mut trials = profile_trial_repository;
        profiles
            .find(profile_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        trials.list_for_profile(profile_id, TRIAL_LIST_LIMIT).await
    }

    #[transactional(profile_trial)]
    pub async fn profile_trial_report(&self, id: TrialId) -> Result<TrialReport, CoreError> {
        let mut trials = profile_trial_repository;
        let trial = trials.find(id).await?.ok_or(CoreError::NotFound)?;
        let leads = trials.list_leads(id).await?;
        let summary = TrialSummary::of(&leads);
        Ok(TrialReport {
            trial,
            leads,
            summary,
        })
    }

    #[transactional(profile_trial)]
    pub async fn fail_profile_trial(
        &self,
        id: TrialId,
        message: &str,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut trials = profile_trial_repository;
        let trial = trials.find(id).await?.ok_or(CoreError::NotFound)?;
        if trial.status.is_final() {
            return Ok(());
        }
        trials
            .transition(
                id,
                &Transition {
                    status: TrialStatus::Failed,
                    error: Some(message.to_owned()),
                    at,
                },
            )
            .await
    }

    pub async fn run_profile_trial<B, S, C, K>(
        &self,
        trial_id: TrialId,
        builder: &B,
        cipher: &S,
        cache: &C,
        settings: EvaluationSettings,
        clock: &K,
    ) -> Result<TrialStatus, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
        let trial = self.find_profile_trial(trial_id).await?;
        if trial.status.is_final() {
            return Ok(trial.status);
        }
        if trial.status == TrialStatus::Pending {
            self.move_profile_trial(
                trial_id,
                Transition {
                    status: TrialStatus::Running,
                    error: None,
                    at: clock.now(),
                },
            )
            .await?;
        }
        let Some((profile, base)) = self.trial_base(&trial).await? else {
            self.fail_profile_trial(trial_id, BASE_REVISION_MISSING, clock.now())
                .await?;
            return Ok(TrialStatus::Failed);
        };
        let active = trial.trial_profile(&profile, &base);
        let calibrations = self.profile_calibrations(&active).await?;
        let (decided, tried) = self.trial_sample(&trial).await?;
        let capacity = usize::from(trial.sample_size.get()).saturating_sub(tried.len());
        let remaining: Vec<DecidedLead> = select_sample(&decided, trial.sample_size)
            .into_iter()
            .filter(|lead| !tried.iter().any(|done| done.lead_id == lead.lead_id))
            .take(capacity)
            .collect();
        let settings = EvaluationSettings {
            budget_cents: trial.budget.cents(),
            ..settings
        };
        let (_, works) = watch::channel(Arc::new(snapshot_of(&active)));
        let budget = RunBudget::new(trial.budget.cents());
        let spent_before = trial.spent_cents;
        let run = &TrialRun {
            services: (builder, cipher, cache),
            clock,
            settings,
            active: &active,
            calibrations: &calibrations,
            providers: &Providers::new(),
            budget: &budget,
            works: &works,
        };
        let mut status = TrialStatus::Done;
        let mut attempts = stream::iter(remaining)
            .map(move |decided| async move { self.try_lead(decided, run).await })
            .buffered(settings.concurrency.get());
        while let Some(tried) = attempts.next().await {
            match tried? {
                Tried::Lead(lead) => {
                    self.record_trial_lead(trial_id, &lead, spent_before + budget.spent_cents())
                        .await?
                }
                Tried::BudgetExhausted => {
                    status = TrialStatus::BudgetExhausted;
                    break;
                }
                Tried::Missing => {}
            }
        }
        drop(attempts);
        self.finish_profile_trial(
            trial_id,
            Transition {
                status,
                error: None,
                at: clock.now(),
            },
            spent_before + budget.spent_cents(),
        )
        .await?;
        info!(trial = %trial_id, status = status.as_str(), "profile trial finished");
        Ok(status)
    }

    async fn try_lead<B, S, C, K>(
        &self,
        decided: DecidedLead,
        run: &TrialRun<'_, B, S, C, K>,
    ) -> Result<Tried, CoreError>
    where
        B: ProviderBuilder,
        S: SecretCipher,
        C: LlmCache,
        K: Clock,
    {
        let Some(lead) = self.lead_of_trial(decided.lead_id).await? else {
            return Ok(Tried::Missing);
        };
        let material = LeadMaterial::load(self, &lead).await?;
        let documents = material.documents();
        let at = run.clock.now();
        let mut record = TrialLead {
            lead_id: lead.id,
            domain: lead.domain.clone(),
            label: decided.label,
            current_verdict: lead.verdict,
            current_confidence: lead.confidence,
            trial_verdict: None,
            trial_confidence: None,
            error: None,
            tried_at: at,
        };
        let attempt = Attempt {
            active: run.active,
            calibrations: run.calibrations,
            lead: &lead,
            documents: &documents,
            facts: &material.facts,
            at,
        };
        let qualified = qualify_in_memory(
            self,
            run.services,
            run.settings,
            &attempt,
            (run.providers, run.budget, run.works),
        )
        .await?;
        let (results, judgement) = match qualified {
            Qualified::Done { results, judgement } => (results, judgement),
            Qualified::BudgetExhausted => return Ok(Tried::BudgetExhausted),
            Qualified::Stopped(reason) => {
                return Ok(stopped(record, reason.as_str(), run.clock));
            }
            Qualified::CallFailed(failure) => {
                return Ok(failed(record, failure.error, run.clock));
            }
        };
        if !results
            .iter()
            .any(|result| result.status == ResultStatus::Valid)
        {
            let error = results
                .iter()
                .find_map(|result| result.error.clone())
                .unwrap_or_else(|| NO_VALID_ANSWER.to_owned());
            return Ok(failed(record, error, run.clock));
        }
        let definition = run.active.revision.definition.definition();
        let found = exclusions(&results, &definition.qualification_agents);
        let scoring_results = if definition.enabled_scoring_agents().next().is_some() {
            let scoring = Scoring {
                active: run.active,
                lead: &lead,
                base: &documents,
                qualification: &results,
                judgement: &judgement,
                found: &found,
                at,
            };
            match score_in_memory(
                self,
                run.services,
                run.settings,
                &scoring,
                (run.providers, run.budget, run.works),
            )
            .await?
            {
                Scored::Done(scored) => scored,
                Scored::BudgetExhausted => return Ok(Tried::BudgetExhausted),
                Scored::Stopped(reason) => {
                    return Ok(stopped(record, reason.as_str(), run.clock));
                }
                Scored::CallFailed(failure) => {
                    return Ok(failed(record, failure.error, run.clock));
                }
            }
        } else {
            Vec::new()
        };
        let decision = decide(
            &definition.scoring_agents,
            &scoring_results,
            run.calibrations,
            found,
        );
        let reading = Reading::of(&decision, &judgement);
        record.trial_verdict = reading.verdict;
        record.trial_confidence = reading.confidence;
        record.tried_at = run.clock.now();
        Ok(Tried::Lead(Box::new(record)))
    }

    #[transactional(profile_trial)]
    async fn find_profile_trial(&self, id: TrialId) -> Result<ProfileTrial, CoreError> {
        let mut trials = profile_trial_repository;
        trials.find(id).await?.ok_or(CoreError::NotFound)
    }

    #[transactional(profile_trial)]
    async fn move_profile_trial(&self, id: TrialId, change: Transition) -> Result<(), CoreError> {
        let mut trials = profile_trial_repository;
        trials.transition(id, &change).await
    }

    #[transactional(profile_trial)]
    async fn record_trial_lead(
        &self,
        trial_id: TrialId,
        lead: &TrialLead,
        spent_cents: f64,
    ) -> Result<(), CoreError> {
        let mut trials = profile_trial_repository;
        trials.record_lead(trial_id, lead).await?;
        trials.record_spend(trial_id, spent_cents).await
    }

    #[transactional(profile_trial)]
    async fn finish_profile_trial(
        &self,
        id: TrialId,
        change: Transition,
        spent_cents: f64,
    ) -> Result<(), CoreError> {
        let mut trials = profile_trial_repository;
        trials.record_spend(id, spent_cents).await?;
        trials.transition(id, &change).await
    }

    #[transactional(profile_trial)]
    async fn trial_sample(
        &self,
        trial: &ProfileTrial,
    ) -> Result<(Vec<DecidedLead>, Vec<TrialLead>), CoreError> {
        let mut trials = profile_trial_repository;
        Ok((
            trials.decided_leads(trial.profile_id).await?,
            trials.list_leads(trial.id).await?,
        ))
    }

    #[transactional(profile)]
    async fn trial_base(
        &self,
        trial: &ProfileTrial,
    ) -> Result<Option<(Profile, ProfileRevision)>, CoreError> {
        let mut profiles = profile_repository;
        let Some(profile) = profiles.find(trial.profile_id).await? else {
            return Ok(None);
        };
        let revision = profiles
            .list_revisions(trial.profile_id)
            .await?
            .into_iter()
            .find(|revision| revision.number == trial.base_revision);
        Ok(revision.map(|revision| (profile, revision)))
    }

    #[transactional(lead)]
    async fn lead_of_trial(&self, id: LeadId) -> Result<Option<Lead>, CoreError> {
        let mut leads = lead_repository;
        leads.find(id).await
    }
}

fn snapshot_of(active: &ActiveProfile) -> WorkSnapshot {
    let mut snapshot = WorkSnapshot::default();
    snapshot.apply(active.profile.id, ProfileAgents::of(active));
    snapshot
}

fn failed<K: Clock>(mut record: TrialLead, error: String, clock: &K) -> Tried {
    record.error = Some(error);
    record.tried_at = clock.now();
    Tried::Lead(Box::new(record))
}

fn stopped<K: Clock>(record: TrialLead, reason: &str, clock: &K) -> Tried {
    failed(record, format!("agent stopped: {reason}"), clock)
}
