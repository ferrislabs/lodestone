use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use common::CoreError;
use lodestone_macros::transactional;

use crate::{
    UserId,
    application::LodestoneUseCase,
    domain::{
        lead::{Lead, ports::AssessmentRepository},
        profile::{ProfileId, RevisionNumber, ports::ProfileRepository},
        qualification::ports::QualificationRepository,
        quality::{
            DecisionKind, DecisionLabel, DecisionSnapshot, LabelFilter, ProfileInfo, QualityReport,
            Rejection, ports::DecisionRepository,
        },
    },
};

pub(crate) struct Decided<'a> {
    pub(crate) kinds: &'a [DecisionKind],
    pub(crate) rejection: Option<&'a Rejection>,
    pub(crate) actor: UserId,
    pub(crate) at: DateTime<Utc>,
}

pub(crate) async fn record_decisions(
    decisions: &mut impl DecisionRepository,
    qualification: &mut impl QualificationRepository,
    profiles: &mut impl ProfileRepository,
    assessments: &mut impl AssessmentRepository,
    lead: &Lead,
    decided: Decided<'_>,
) -> Result<(), CoreError> {
    let results = qualification.current_results(lead.id).await?;
    let assessment = assessments.list_for_lead(lead.id).await?.into_iter().next();
    let scoring = qualification.scorings(&[lead.id]).await?.remove(&lead.id);
    let profile_revision = match results.first() {
        Some(result) => result.profile_revision,
        None => profiles
            .list_revisions(lead.profile_id)
            .await?
            .last()
            .map_or(RevisionNumber::FIRST, |revision| revision.number),
    };
    let snapshot = DecisionSnapshot::capture(lead, scoring.as_ref(), &results, assessment.as_ref());
    for kind in decided.kinds {
        decisions
            .record(&DecisionLabel {
                id: common::generate_uuid_v7(),
                lead_id: lead.id,
                profile_id: lead.profile_id,
                profile_revision,
                kind: *kind,
                decided_by: Some(decided.actor),
                decided_at: decided.at,
                snapshot: snapshot.clone(),
                rejection: decided.rejection.filter(|_| !kind.is_approval()).cloned(),
            })
            .await?;
    }
    Ok(())
}

impl LodestoneUseCase {
    #[transactional(decision, profile)]
    pub async fn quality_report(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        profile_id: Option<ProfileId>,
    ) -> Result<QualityReport, CoreError> {
        let mut decisions = decision_repository;
        let mut profiles = profile_repository;

        if let Some(profile_id) = profile_id
            && profiles.find(profile_id).await?.is_none()
        {
            return Err(CoreError::NotFound);
        }
        let filter = LabelFilter {
            from,
            to,
            profile_id,
        };
        let labels = decisions.list(&filter).await?;
        let verdicts = decisions.verdict_counts(&filter).await?;
        let mut infos = Vec::new();
        let mut agent_names = BTreeMap::new();
        for profile in profiles.list().await? {
            if profile_id.is_some_and(|selected| selected != profile.id) {
                continue;
            }
            let revisions = profiles.list_revisions(profile.id).await?;
            for revision in &revisions {
                for agent in &revision.definition.definition().qualification_agents {
                    agent_names.insert(agent.id.as_str().to_owned(), agent.name.clone());
                }
            }
            let thresholds = revisions
                .last()
                .map(|revision| revision.definition.definition().verdict)
                .unwrap_or_default();
            infos.push(ProfileInfo {
                id: profile.id,
                name: profile.name.as_str().to_owned(),
                thresholds,
            });
        }
        Ok(QualityReport::compute(
            &labels,
            verdicts,
            &infos,
            profile_id,
            &agent_names,
        ))
    }
}
