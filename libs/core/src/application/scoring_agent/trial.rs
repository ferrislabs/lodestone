use chrono::{DateTime, Utc};
use common::CoreError;
use futures_util::{StreamExt, stream};

use super::{AgentOutcome, Scorer, Subject, scoring_inputs};
use crate::{
    application::{
        LodestoneUseCase,
        evaluation::{CallFailure, EvaluationSettings, ProviderBuilder, Providers, TrialWorks},
        llm::gateway::RunBudget,
    },
    domain::{
        agent_work::{AgentWork, CancelReason, WorkKind, WorkTarget},
        lead::Lead,
        llm::{UntrustedDocument, cache::LlmCache, provider::SecretCipher},
        profile::{ActiveProfile, definition::ScoringAgent},
        qualification::{Exclusion, Judgement, QualificationResult},
        scoring_agent::ScoringResult,
    },
};

pub(crate) struct Scoring<'a> {
    pub(crate) active: &'a ActiveProfile,
    pub(crate) lead: &'a Lead,
    pub(crate) base: &'a [UntrustedDocument<'a>],
    pub(crate) qualification: &'a [QualificationResult],
    pub(crate) judgement: &'a Judgement,
    pub(crate) found: &'a [Exclusion],
    pub(crate) at: DateTime<Utc>,
}

pub(crate) enum Scored {
    Done(Vec<ScoringResult>),
    CallFailed(CallFailure),
    Stopped(CancelReason),
    BudgetExhausted,
}

pub(crate) async fn score_in_memory<B, S, C>(
    usecase: &LodestoneUseCase,
    (builder, cipher, cache): (&B, &S, &C),
    settings: EvaluationSettings,
    scoring: &Scoring<'_>,
    (providers, budget, works): (&Providers<B::Provider>, &RunBudget, &TrialWorks),
) -> Result<Scored, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
    C: LlmCache,
{
    let scorer = &Scorer {
        builder,
        cipher,
        cache,
        settings,
    };
    let active = scoring.active;
    let definition = active.revision.definition.definition();
    let inputs = scoring_inputs(
        definition,
        scoring.qualification,
        scoring.judgement,
        scoring.found,
    );
    let (documents, grounding) = inputs.documents(scoring.base);
    let subject = &Subject {
        active,
        lead: scoring.lead,
        documents: &documents,
        grounding: &grounding,
        fresh: false,
        at: scoring.at,
    };
    let agents: &Vec<&ScoringAgent> = &definition.enabled_scoring_agents().collect();
    let mut outcomes = stream::iter(0..agents.len())
        .map(move |index| async move {
            let agent = agents[index];
            let work = AgentWork::new(
                active.profile.id,
                active.revision.number,
                WorkTarget::new(WorkKind::Scoring, &agent.id),
                works.clone(),
            );
            scorer
                .score_agent(usecase, subject, agent, &work, providers, budget)
                .await
        })
        .buffered(settings.concurrency.get());
    let mut results = Vec::new();
    while let Some(outcome) = outcomes.next().await {
        match outcome? {
            AgentOutcome::BudgetExhausted => return Ok(Scored::BudgetExhausted),
            AgentOutcome::Stopped(reason) => return Ok(Scored::Stopped(reason)),
            AgentOutcome::CallFailed(failure) => return Ok(Scored::CallFailed(failure)),
            AgentOutcome::Result(result) => results.push(*result),
        }
    }
    Ok(Scored::Done(results))
}
