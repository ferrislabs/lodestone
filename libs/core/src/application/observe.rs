use chrono::{DateTime, Utc};

use crate::{
    application::{
        LodestoneUseCase,
        llm::gateway::{LlmCallError, RunBudget, Structured},
    },
    domain::{
        llm::{LlmError, provider::Tier},
        profile::ProfileId,
        run::{Run, RunOutcome},
        values::{SignalKind, SourceName},
    },
};
use telemetry::{LlmCall, LlmIdentity};

pub(crate) const BUDGET_EXHAUSTED: &str = "llm budget exhausted";
const UNKNOWN: &str = "unknown";

pub(crate) fn call_outcome(error: &LlmCallError) -> &'static str {
    match error {
        LlmCallError::BudgetExhausted => "budget_exhausted",
        LlmCallError::InvalidOutput(_) => "invalid_output",
        LlmCallError::Schema(_) => "schema_error",
        LlmCallError::Provider(LlmError::Timeout) => "timeout",
        LlmCallError::Provider(LlmError::RateLimited { .. }) => "rate_limited",
        LlmCallError::Provider(_) => "provider_error",
    }
}

pub(crate) fn run_outcome(outcome: &RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Succeeded { .. } => "succeeded",
        RunOutcome::Failed { .. } => "failed",
    }
}

impl LodestoneUseCase {
    pub(crate) fn observe_run(&self, run: &Run, outcome: &RunOutcome, finished_at: DateTime<Utc>) {
        let source = run.source.as_str();
        let volume = match outcome {
            RunOutcome::Succeeded { volume, .. } => *volume,
            RunOutcome::Failed { volume, error, .. } => {
                if error == BUDGET_EXHAUSTED {
                    self.metrics.source_error(source, "budget");
                }
                *volume
            }
        };
        let elapsed = (finished_at - run.started_at).to_std().unwrap_or_default();
        self.metrics.source_run_finished(
            source,
            &run.profile_id.0.to_string(),
            run_outcome(outcome),
            volume,
            elapsed,
        );
    }

    pub(crate) fn observe_signals(
        &self,
        profile: ProfileId,
        source: SourceName,
        recorded: &[SignalKind],
    ) {
        let profile = profile.0.to_string();
        let mut tally: Vec<(&SignalKind, u64)> = Vec::new();
        for kind in recorded {
            match tally.iter_mut().find(|(seen, _)| *seen == kind) {
                Some((_, count)) => *count += 1,
                None => tally.push((kind, 1)),
            }
        }
        for (kind, count) in tally {
            self.metrics
                .signals_recorded(source.as_str(), &profile, kind.as_str(), count);
        }
    }

    pub(crate) fn observe_llm<T>(
        &self,
        purpose: Tier,
        profile: ProfileId,
        spent_before: u64,
        budget: &RunBudget,
        result: &Result<Structured<T>, LlmCallError>,
    ) {
        let profile = profile.0.to_string();
        if matches!(result, Err(LlmCallError::BudgetExhausted)) {
            self.metrics
                .llm_budget_exhausted(purpose.as_str(), &profile);
            return;
        }
        let (provider_kind, model) = self.llm.as_ref().map_or((UNKNOWN, UNKNOWN), |context| {
            (context.kind().as_str(), context.model())
        });
        let identity = LlmIdentity {
            purpose: purpose.as_str(),
            provider_kind,
            model,
            profile: &profile,
        };
        let cost_microcents = budget.spent_microcents().saturating_sub(spent_before);
        let call = match result {
            Ok(structured) => LlmCall {
                identity,
                outcome: "succeeded",
                cached: structured.from_cache,
                input_tokens: structured.usage.input_tokens,
                output_tokens: structured.usage.output_tokens,
                cost_microcents,
            },
            Err(error) => LlmCall {
                identity,
                outcome: call_outcome(error),
                cached: false,
                input_tokens: 0,
                output_tokens: 0,
                cost_microcents,
            },
        };
        self.metrics.llm_call(&call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_outcomes_are_bounded_labels() {
        assert_eq!(
            call_outcome(&LlmCallError::Provider(LlmError::Timeout)),
            "timeout"
        );
        assert_eq!(
            call_outcome(&LlmCallError::Provider(LlmError::RateLimited {
                retry_after: None
            })),
            "rate_limited"
        );
        assert_eq!(
            call_outcome(&LlmCallError::Provider(LlmError::Status { status: 500 })),
            "provider_error"
        );
        assert_eq!(
            call_outcome(&LlmCallError::InvalidOutput("x".to_owned())),
            "invalid_output"
        );
        assert_eq!(
            call_outcome(&LlmCallError::Schema("x".to_owned())),
            "schema_error"
        );
        assert_eq!(
            call_outcome(&LlmCallError::BudgetExhausted),
            "budget_exhausted"
        );
    }

    #[test]
    fn run_outcomes_are_bounded_labels() {
        assert_eq!(
            run_outcome(&RunOutcome::Succeeded {
                volume: 0,
                signals: 0
            }),
            "succeeded"
        );
        assert_eq!(
            run_outcome(&RunOutcome::Failed {
                volume: 0,
                signals: 0,
                error: String::new()
            }),
            "failed"
        );
    }
}
