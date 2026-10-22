use std::borrow::Cow;

use chrono::{DateTime, Utc};
use common::{CoreError, generate_uuid_v7};
use lodestone_macros::transactional;
use serde_json::json;
use tracing::warn;

use crate::{
    application::{
        LodestoneUseCase,
        sources::{Collected, LeadCreation, SourceRunError, collect_until},
    },
    domain::{
        agent_work::{AgentWork, CancelReason, WorkKind, WorkTarget},
        dropped::{DropReason, DroppedCandidate, DroppedId, dropped_excerpts},
        enrichment::EnrichmentStep,
        job::ports::Clock,
        profile::{
            ActiveProfile, ProfileId,
            definition::{AgentId, ProfileDefinition, SearchAgent},
        },
        run::{Run, RunId, RunOutcome, ports::RunRepository},
        source::{Observation, SourceError},
        triage::{Candidate, Route, excerpts, failing_filter, route},
        values::{SourceName, UnitInterval},
    },
};

pub const MAX_DROPPED_PER_TARGET: usize = 200;
const TITLE_MAX_CHARS: usize = 200;
const RULES_STRENGTH: f64 = 0.7;
const AMBIGUOUS_STRENGTH: f64 = 0.5;

pub trait CandidateSource: Send + Sync {
    fn name(&self) -> SourceName;

    fn collect(
        &self,
        profiles: &[ActiveProfile],
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Candidate>, SourceError>> + Send;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriageReport {
    pub candidates: u64,
    pub accepted_by_rules: u64,
    pub accepted_ambiguous: u64,
    pub rejected_by_rules: u64,
    pub rejected_by_exclusions: u64,
    pub rejected_by_filters: u64,
    pub dropped_kept: u64,
    pub dropped_not_kept: u64,
    pub skipped_no_lead: u64,
    pub runs: u64,
    pub cancelled: u64,
    pub cancel_reason: Option<CancelReason>,
}

impl TriageReport {
    pub fn accepted(&self) -> u64 {
        self.accepted_by_rules + self.accepted_ambiguous
    }

    pub fn all_cancelled(&self) -> Option<CancelReason> {
        (self.runs > 0 && self.cancelled == self.runs)
            .then_some(self.cancel_reason)
            .flatten()
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn strength(value: f64) -> Result<UnitInterval, CoreError> {
    UnitInterval::new(value).map_err(|_| CoreError::Internal("invalid strength".to_owned()))
}

fn short_title(candidate: &Candidate) -> String {
    candidate.title.chars().take(TITLE_MAX_CHARS).collect()
}

fn candidate_text(candidate: &Candidate) -> String {
    format!("{}\n\n{}", candidate.title, candidate.text)
}

struct KeywordTarget<'a> {
    agent: Option<&'a AgentId>,
    rules: Cow<'a, SearchAgent>,
}

impl KeywordTarget<'_> {
    fn work_target(&self) -> WorkTarget {
        match self.agent {
            Some(agent) => WorkTarget::new(WorkKind::Sourcing, agent),
            None => WorkTarget::enrichment(EnrichmentStep::CompanyJobs),
        }
    }
}

fn keyword_targets(definition: &ProfileDefinition, source: SourceName) -> Vec<KeywordTarget<'_>> {
    let mut targets: Vec<KeywordTarget<'_>> = definition
        .enabled_search_agents(source)
        .map(|agent| KeywordTarget {
            agent: Some(&agent.id),
            rules: Cow::Borrowed(agent),
        })
        .collect();
    if source == SourceName::Ats
        && let Some(rules) = definition.company_jobs_rules()
    {
        targets.push(KeywordTarget {
            agent: None,
            rules: Cow::Owned(rules),
        });
    }
    targets
}

fn observation(
    candidate: &Candidate,
    agent: Option<&AgentId>,
    matched: &[String],
    route: &str,
    strength: f64,
) -> Result<Observation, CoreError> {
    let mut raw = json!({
        "title": candidate.title,
        "matched": matched,
        "route": route,
        "details": candidate.details,
    });
    let passages = excerpts(&candidate.text, matched);
    if !passages.is_empty() {
        raw["excerpts"] = json!(passages);
    }
    Ok(Observation {
        domain: candidate.domain.clone(),
        kind: candidate.kind.clone(),
        value: Some(short_title(candidate)),
        raw,
        provenance: candidate.provenance.clone(),
        strength: self::strength(strength)?,
        observed_at: candidate.observed_at,
        agent: agent.cloned(),
    })
}

struct DropScope<'a> {
    profile_id: ProfileId,
    agent: Option<&'a AgentId>,
    source: SourceName,
    at: DateTime<Utc>,
}

#[derive(Default)]
struct Triaged {
    observations: Vec<Observation>,
    dropped: Vec<DroppedCandidate>,
}

impl Triaged {
    fn has_room(&self, report: &mut TriageReport) -> bool {
        let room = self.dropped.len() < MAX_DROPPED_PER_TARGET;
        if !room {
            report.dropped_not_kept += 1;
        }
        room
    }

    fn drop_candidate(
        &mut self,
        scope: &DropScope<'_>,
        candidate: &Candidate,
        reason: DropReason,
        keywords: (Vec<String>, Vec<String>),
    ) {
        let (matched, excluded) = keywords;
        self.dropped.push(DroppedCandidate {
            id: DroppedId(generate_uuid_v7()),
            profile_id: scope.profile_id,
            agent: scope.agent.cloned(),
            source: scope.source,
            kind: candidate.kind.clone(),
            domain: candidate.domain.clone(),
            title: candidate.title.clone(),
            excerpts: dropped_excerpts(&candidate.text, reason, &matched, &excluded),
            provenance: candidate.provenance.clone(),
            observed_at: candidate.observed_at,
            reason,
            matched,
            excluded,
            details: candidate.details.clone(),
            dropped_at: scope.at,
            review: None,
            reviewed_at: None,
            reviewed_by: None,
        });
    }
}

fn keywords_of(route: Route) -> (Vec<String>, Vec<String>) {
    match route {
        Route::Reject => (Vec::new(), Vec::new()),
        Route::Excluded { matched, excluded } => (matched, excluded),
        Route::Accept { matched } | Route::Ambiguous { matched } => (matched, Vec::new()),
    }
}

fn decide(
    target: &KeywordTarget<'_>,
    scope: &DropScope<'_>,
    candidates: &[Candidate],
    report: &mut TriageReport,
) -> Result<Triaged, CoreError> {
    let agent = target.rules.as_ref();
    let mut triaged = Triaged::default();
    for candidate in candidates {
        if let Some(filter) = failing_filter(&agent.filters, candidate) {
            report.rejected_by_filters += 1;
            if triaged.has_room(report) {
                let keywords = keywords_of(route(&candidate_text(candidate), agent));
                triaged.drop_candidate(scope, candidate, DropReason::from(filter), keywords);
            }
            continue;
        }
        match route(&candidate_text(candidate), agent) {
            Route::Reject => {
                report.rejected_by_rules += 1;
                if triaged.has_room(report) {
                    triaged.drop_candidate(
                        scope,
                        candidate,
                        DropReason::NoKeyword,
                        (Vec::new(), Vec::new()),
                    );
                }
            }
            Route::Excluded { matched, excluded } => {
                report.rejected_by_exclusions += 1;
                if triaged.has_room(report) {
                    triaged.drop_candidate(
                        scope,
                        candidate,
                        DropReason::Exclusion,
                        (matched, excluded),
                    );
                }
            }
            Route::Accept { matched } => {
                report.accepted_by_rules += 1;
                triaged.observations.push(observation(
                    candidate,
                    target.agent,
                    &matched,
                    "keywords",
                    RULES_STRENGTH,
                )?);
            }
            Route::Ambiguous { matched } => {
                report.accepted_ambiguous += 1;
                triaged.observations.push(observation(
                    candidate,
                    target.agent,
                    &matched,
                    "keywords_ambiguous",
                    AMBIGUOUS_STRENGTH,
                )?);
            }
        }
    }
    Ok(triaged)
}

impl LodestoneUseCase {
    pub async fn run_triage<S, K>(
        &self,
        source: &S,
        clock: &K,
    ) -> Result<TriageReport, SourceRunError>
    where
        S: CandidateSource,
        K: Clock,
    {
        let name = source.name();
        let profiles: Vec<ActiveProfile> = self
            .refreshed_active_profiles()
            .await?
            .into_iter()
            .filter(|active| {
                !keyword_targets(active.revision.definition.definition(), name).is_empty()
            })
            .collect();
        let targets: Vec<(&ActiveProfile, KeywordTarget<'_>)> = profiles
            .iter()
            .flat_map(|active| {
                keyword_targets(active.revision.definition.definition(), name)
                    .into_iter()
                    .map(move |target| (active, target))
            })
            .collect();
        if targets.is_empty() {
            return Ok(TriageReport::default());
        }
        let works: Vec<AgentWork> = targets
            .iter()
            .map(|(active, target)| self.agent_work(active, target.work_target()))
            .collect();

        let started_at = clock.now();
        let runs: Vec<Run> = targets
            .iter()
            .map(|(active, target)| {
                Run::start(
                    RunId(generate_uuid_v7()),
                    active.profile.id,
                    name,
                    target.agent.cloned(),
                    started_at,
                )
            })
            .collect();
        self.insert_triage_runs(&runs).await?;

        let collected = match collect_until(&works, source.collect(&profiles, started_at)).await {
            Collected::Items(collected) => collected,
            Collected::Stopped(reason) => {
                for (run, work) in runs.iter().zip(&works) {
                    let outcome = RunOutcome::Cancelled {
                        volume: 0,
                        signals: 0,
                        reason: work.stop_reason().unwrap_or(reason),
                    };
                    self.finish_triage_run(run.id, &outcome, clock.now())
                        .await?;
                }
                return Ok(TriageReport {
                    runs: count(runs.len()),
                    cancelled: count(runs.len()),
                    cancel_reason: Some(reason),
                    ..TriageReport::default()
                });
            }
        };
        let candidates = match collected {
            Ok(candidates) => candidates,
            Err(error) => {
                let outcome = RunOutcome::Failed {
                    volume: 0,
                    signals: 0,
                    error: error.to_string(),
                };
                for run in &runs {
                    self.finish_triage_run(run.id, &outcome, clock.now())
                        .await?;
                }
                return Err(error.into());
            }
        };

        let volume = count(candidates.len());
        let mut report = TriageReport {
            candidates: volume,
            runs: count(runs.len()),
            ..TriageReport::default()
        };
        let mut first_error: Option<CoreError> = None;
        for (((active, target), run), work) in targets.iter().zip(&runs).zip(&works) {
            let scope = DropScope {
                profile_id: active.profile.id,
                agent: target.agent,
                source: name,
                at: clock.now(),
            };
            let Triaged {
                observations,
                dropped,
            } = match decide(target, &scope, &candidates, &mut report) {
                Ok(triaged) => triaged,
                Err(error) => {
                    let outcome = RunOutcome::Failed {
                        volume,
                        signals: 0,
                        error: error.to_string(),
                    };
                    self.finish_triage_run(run.id, &outcome, clock.now())
                        .await?;
                    first_error.get_or_insert(error);
                    continue;
                }
            };
            let creation = match target.agent {
                Some(_) => LeadCreation::Create,
                None => LeadCreation::ExistingOnly,
            };
            let recorded = self
                .record_until_stopped(
                    active,
                    name,
                    observations
                        .into_iter()
                        .map(|observation| vec![observation]),
                    work,
                    creation,
                    clock,
                )
                .await;
            match &recorded {
                Ok(recorded) if recorded.stopped.is_none() => {
                    self.keep_dropped(&dropped, &mut report).await;
                }
                _ => report.dropped_not_kept += count(dropped.len()),
            }
            let outcome = match &recorded {
                Ok(recorded) => recorded.outcome(volume),
                Err(error) => RunOutcome::Failed {
                    volume,
                    signals: 0,
                    error: error.to_string(),
                },
            };
            self.finish_triage_run(run.id, &outcome, clock.now())
                .await?;
            match recorded {
                Ok(recorded) => {
                    report.skipped_no_lead += recorded.skipped.no_lead;
                    if let Some(reason) = recorded.stopped {
                        report.cancelled += 1;
                        report.cancel_reason.get_or_insert(reason);
                    }
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error.into()),
            None => Ok(report),
        }
    }

    async fn keep_dropped(&self, dropped: &[DroppedCandidate], report: &mut TriageReport) {
        if dropped.is_empty() {
            return;
        }
        match self.store_dropped(dropped).await {
            Ok(stored) => report.dropped_kept += stored,
            Err(error) => {
                warn!(%error, dropped = dropped.len(), "dropped candidates could not be stored");
                report.dropped_not_kept += count(dropped.len());
            }
        }
    }

    #[transactional(run)]
    async fn insert_triage_runs(&self, runs: &[Run]) -> Result<(), CoreError> {
        let mut repo = run_repository;
        for run in runs {
            repo.insert(run).await?;
        }
        Ok(())
    }

    #[transactional(run)]
    async fn finish_triage_run(
        &self,
        id: RunId,
        outcome: &RunOutcome,
        at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        let mut repo = run_repository;
        repo.finish(id, outcome, at).await?;
        Ok(())
    }
}
