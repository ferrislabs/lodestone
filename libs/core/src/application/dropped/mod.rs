use std::num::NonZeroU16;

use chrono::{DateTime, Duration, Utc};
use common::CoreError;
use lodestone_macros::transactional;
use serde_json::json;

use crate::{
    NormalizedDomain, UserId,
    application::{LodestoneUseCase, sources::LeadCreation},
    domain::{
        dropped::{
            DropReview, DroppedCandidate, DroppedFilter, DroppedId,
            ports::DroppedCandidateRepository,
        },
        job::{JobKind, NewJob},
        lead::{LeadId, ports::LeadRepository},
        profile::{ProfileId, ports::ProfileRepository},
        source::Observation,
        suppression::ports::SuppressionRepository,
        values::UnitInterval,
    },
};

pub const DROPPED_PURGE_JOB_KIND: &str = "triage.purge_dropped";
pub const DROPPED_RETENTION_DAYS: i64 = 30;
const PURGE_DEDUP_KEY: &str = "triage:purge_dropped";
const PURGE_MAX_ATTEMPTS: NonZeroU16 = match NonZeroU16::new(3) {
    Some(attempts) => attempts,
    None => NonZeroU16::MIN,
};
const PURGE_INTERVAL_HOURS: i64 = 24;
const RECOVERED_ROUTE: &str = "recovered";
const RECOVERED_STRENGTH: f64 = 0.5;
const TITLE_MAX_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewedDrop {
    pub candidate: DroppedCandidate,
    pub lead_id: Option<LeadId>,
}

pub fn dropped_purge_job(run_at: DateTime<Utc>) -> Result<NewJob, CoreError> {
    let kind = JobKind::parse(DROPPED_PURGE_JOB_KIND)
        .map_err(|_| CoreError::Internal("invalid dropped purge job kind".to_owned()))?;
    Ok(NewJob {
        kind,
        payload: json!({}),
        run_at,
        max_attempts: PURGE_MAX_ATTEMPTS,
        dedup_key: Some(PURGE_DEDUP_KEY.to_owned()),
    })
}

pub fn next_dropped_purge(now: DateTime<Utc>) -> DateTime<Utc> {
    now + Duration::hours(PURGE_INTERVAL_HOURS)
}

fn already_reviewed() -> CoreError {
    CoreError::Conflict("dropped candidate already reviewed".to_owned())
}

fn recovery(candidate: &DroppedCandidate) -> Result<Observation, CoreError> {
    let strength = UnitInterval::new(RECOVERED_STRENGTH)
        .map_err(|_| CoreError::Internal("invalid strength".to_owned()))?;
    Ok(Observation {
        domain: candidate.domain.clone(),
        kind: candidate.kind.clone(),
        value: Some(candidate.title.chars().take(TITLE_MAX_CHARS).collect()),
        raw: json!({
            "title": candidate.title,
            "matched": candidate.matched,
            "route": RECOVERED_ROUTE,
            "details": candidate.details,
            "excerpts": candidate.excerpts,
        }),
        provenance: candidate.provenance.clone(),
        strength,
        observed_at: candidate.observed_at,
        agent: candidate.agent.clone(),
    })
}

impl LodestoneUseCase {
    #[transactional(profile, dropped)]
    pub async fn list_dropped(
        &self,
        profile_id: ProfileId,
        filter: &DroppedFilter,
        limit: u32,
        random: bool,
    ) -> Result<Vec<DroppedCandidate>, CoreError> {
        let mut profiles = profile_repository;
        let mut dropped = dropped_repository;
        if profiles.find(profile_id).await?.is_none() {
            return Err(CoreError::NotFound);
        }
        dropped.list(profile_id, filter, limit, random).await
    }

    pub async fn review_dropped(
        &self,
        id: DroppedId,
        review: DropReview,
        actor: UserId,
        now: DateTime<Utc>,
    ) -> Result<ReviewedDrop, CoreError> {
        let mut candidate = self.find_dropped(id).await?.ok_or(CoreError::NotFound)?;
        if candidate.review.is_some() {
            return Err(already_reviewed());
        }
        let mut lead_id = None;
        if review == DropReview::ShouldHaveKept {
            let active = self
                .list_active_profiles()
                .await?
                .into_iter()
                .find(|active| active.profile.id == candidate.profile_id)
                .ok_or_else(|| {
                    CoreError::Conflict(
                        "the profile is not active, activate it to recover this candidate"
                            .to_owned(),
                    )
                })?;
            let creation = match candidate.agent {
                Some(_) => LeadCreation::Create,
                None => LeadCreation::ExistingOnly,
            };
            self.record_observations_with(
                &active,
                candidate.source,
                &[recovery(&candidate)?],
                now,
                creation,
            )
            .await?;
            lead_id = self
                .recovered_lead(candidate.profile_id, &candidate.domain)
                .await?;
        }
        if !self.set_dropped_review(id, review, actor, now).await? {
            return Err(already_reviewed());
        }
        candidate.review = Some(review);
        candidate.reviewed_at = Some(now);
        candidate.reviewed_by = Some(actor);
        Ok(ReviewedDrop { candidate, lead_id })
    }

    #[transactional(dropped)]
    pub async fn purge_dropped(&self, now: DateTime<Utc>) -> Result<u64, CoreError> {
        let mut dropped = dropped_repository;
        dropped
            .purge(now - Duration::days(DROPPED_RETENTION_DAYS))
            .await
    }

    #[transactional(dropped)]
    pub(crate) async fn store_dropped(
        &self,
        candidates: &[DroppedCandidate],
    ) -> Result<u64, CoreError> {
        let mut dropped = dropped_repository;
        dropped.record(candidates).await
    }

    #[transactional(dropped)]
    async fn find_dropped(&self, id: DroppedId) -> Result<Option<DroppedCandidate>, CoreError> {
        let mut dropped = dropped_repository;
        dropped.find(id).await
    }

    #[transactional(dropped)]
    async fn set_dropped_review(
        &self,
        id: DroppedId,
        review: DropReview,
        actor: UserId,
        at: DateTime<Utc>,
    ) -> Result<bool, CoreError> {
        let mut dropped = dropped_repository;
        dropped.review(id, review, Some(actor), at).await
    }

    #[transactional(lead, suppression)]
    async fn recovered_lead(
        &self,
        profile_id: ProfileId,
        domain: &NormalizedDomain,
    ) -> Result<Option<LeadId>, CoreError> {
        let mut leads = lead_repository;
        let mut suppressions = suppression_repository;
        if suppressions.is_domain_suppressed(domain).await? {
            return Ok(None);
        }
        Ok(leads
            .find_by_domain(profile_id, domain)
            .await?
            .map(|lead| lead.id))
    }
}
