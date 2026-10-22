use std::sync::Arc;

use chrono::{DateTime, Utc};
use common::CoreError;
use futures_util::{StreamExt, stream};
use tokio::sync::watch;

use super::{
    AgentOutcome, CallFailure, EvaluationSettings, Evaluator, ProviderBuilder, Providers, RunPlan,
    Subject, judgement_of,
};
use crate::{
    application::{LodestoneUseCase, llm::gateway::RunBudget, timeline::round_key},
    domain::{
        agent_work::{AgentWork, CancelReason, WorkKind, WorkSnapshot, WorkTarget},
        calibration::Calibrations,
        facts::LeadFacts,
        lead::Lead,
        llm::{UntrustedDocument, cache::LlmCache, provider::SecretCipher},
        profile::{ActiveProfile, definition::QualificationAgent},
        qualification::{Judgement, QualificationResult},
    },
};

pub(crate) type TrialWorks = watch::Receiver<Arc<WorkSnapshot>>;

pub(crate) struct Attempt<'a> {
    pub(crate) active: &'a ActiveProfile,
    pub(crate) calibrations: &'a Calibrations,
    pub(crate) lead: &'a Lead,
    pub(crate) documents: &'a [UntrustedDocument<'a>],
    pub(crate) facts: &'a LeadFacts,
    pub(crate) at: DateTime<Utc>,
}

pub(crate) enum Qualified {
    Done {
        results: Vec<QualificationResult>,
        judgement: Judgement,
    },
    CallFailed(CallFailure),
    Stopped(CancelReason),
    BudgetExhausted,
}

pub(crate) async fn qualify_in_memory<B, S, C>(
    usecase: &LodestoneUseCase,
    (builder, cipher, cache): (&B, &S, &C),
    settings: EvaluationSettings,
    attempt: &Attempt<'_>,
    (providers, budget, works): (&Providers<B::Provider>, &RunBudget, &TrialWorks),
) -> Result<Qualified, CoreError>
where
    B: ProviderBuilder,
    S: SecretCipher,
    C: LlmCache,
{
    let evaluator = &Evaluator {
        builder,
        cipher,
        cache,
        settings,
    };
    let active = attempt.active;
    let definition = active.revision.definition.definition();
    let subject = &Subject {
        active,
        definition,
        calibrations: attempt.calibrations,
        lead: attempt.lead,
        documents: attempt.documents,
        fresh: false,
        round: round_key(attempt.lead.qualified_at),
        at: attempt.at,
    };
    let agents: &Vec<&QualificationAgent> = &definition
        .qualification_agents
        .iter()
        .filter(|agent| agent.enabled)
        .collect();
    let mut outcomes = stream::iter(0..agents.len())
        .map(move |index| async move {
            let agent = agents[index];
            let work = AgentWork::new(
                active.profile.id,
                active.revision.number,
                WorkTarget::new(WorkKind::Qualification, &agent.id),
                works.clone(),
            );
            evaluator
                .evaluate_agent(
                    usecase,
                    subject,
                    agent,
                    (RunPlan::primary(agent), &work),
                    providers,
                    budget,
                )
                .await
        })
        .buffered(settings.concurrency.get());
    let mut results = Vec::new();
    let mut opinions = Vec::new();
    while let Some(outcome) = outcomes.next().await {
        match outcome? {
            AgentOutcome::BudgetExhausted => return Ok(Qualified::BudgetExhausted),
            AgentOutcome::Stopped(reason) => return Ok(Qualified::Stopped(reason)),
            AgentOutcome::CallFailed(failure) => return Ok(Qualified::CallFailed(failure)),
            AgentOutcome::Result(result, opinion) => {
                results.push(*result);
                opinions.extend(opinion);
            }
        }
    }
    let judgement = judgement_of(definition, attempt.facts, &opinions, attempt.at);
    Ok(Qualified::Done { results, judgement })
}
