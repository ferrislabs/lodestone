use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::domain::{
    lead::{Score, Signal},
    profile::definition::Scoring,
};

pub const DISTINCT_SIGNALS_FACTOR: &str = "distinct_signals";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreContribution {
    pub signal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub weight: f64,
    pub measure: f64,
    pub contribution: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScoreBreakdown {
    pub total: Score,
    pub contributions: Vec<ScoreContribution>,
}

fn strongest_match(signals: &[Signal], kind: &str, value: Option<&str>) -> f64 {
    signals
        .iter()
        .filter(|signal| signal.kind.as_str() == kind)
        .filter(|signal| value.is_none_or(|wanted| signal.value.as_deref() == Some(wanted)))
        .map(|signal| signal.strength.get())
        .fold(0.0, f64::max)
}

fn distinct_kinds(signals: &[Signal]) -> f64 {
    let kinds: BTreeSet<&str> = signals.iter().map(|signal| signal.kind.as_str()).collect();
    f64::from(u32::try_from(kinds.len()).unwrap_or(u32::MAX))
}

pub fn score(signals: &[Signal], scoring: &Scoring) -> ScoreBreakdown {
    let contributions: Vec<ScoreContribution> = scoring
        .factors
        .iter()
        .map(|factor| {
            let measure = if factor.signal == DISTINCT_SIGNALS_FACTOR {
                distinct_kinds(signals)
            } else {
                strongest_match(signals, &factor.signal, factor.value.as_deref())
            };
            ScoreContribution {
                signal: factor.signal.clone(),
                value: factor.value.clone(),
                weight: factor.weight,
                measure,
                contribution: factor.weight * measure,
            }
        })
        .collect();
    let total: f64 = contributions.iter().map(|c| c.contribution).sum();
    ScoreBreakdown {
        total: Score::new(total).unwrap_or_default(),
        contributions,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::domain::{
        lead::{LeadId, SignalId},
        profile::definition::ScoringFactor,
        values::{Provenance, SignalKind, SourceName, UnitInterval},
    };

    fn signal(kind: &str, value: Option<&str>, strength: f64) -> Signal {
        Signal {
            id: SignalId(Uuid::nil()),
            lead_id: LeadId(Uuid::nil()),
            kind: SignalKind::parse(kind).unwrap(),
            value: value.map(str::to_owned),
            source: SourceName::HttpProbe,
            raw: serde_json::Value::Null,
            provenance: Provenance::parse("https://sso.acme.com/").unwrap(),
            strength: UnitInterval::new(strength).unwrap(),
            observed_at: Utc::now(),
            agent: None,
        }
    }

    fn factor(signal: &str, value: Option<&str>, weight: f64) -> ScoringFactor {
        ScoringFactor {
            signal: signal.to_owned(),
            value: value.map(str::to_owned),
            weight,
        }
    }

    fn iam() -> Scoring {
        Scoring {
            factors: vec![
                factor("product_detected", Some("keycloak"), 30.0),
                factor("debt_marker", Some("legacy_auth_path"), 15.0),
                factor("saas_provider", None, 10.0),
                factor("job_offer", None, 25.0),
                factor(DISTINCT_SIGNALS_FACTOR, None, 5.0),
            ],
        }
    }

    #[test]
    fn combination_of_signals_adds_up_with_breakdown() {
        let signals = [
            signal("product_detected", Some("keycloak"), 0.9),
            signal("debt_marker", Some("legacy_auth_path"), 0.6),
            signal("job_offer", None, 1.0),
        ];
        let breakdown = score(&signals, &iam());
        let expected = 30.0 * 0.9 + 15.0 * 0.6 + 25.0 + 5.0 * 3.0;
        assert!((breakdown.total.get() - expected).abs() < 1e-9);
        assert_eq!(breakdown.contributions.len(), 5);
        assert_eq!(breakdown.contributions[2].measure, 0.0);
    }

    #[test]
    fn value_must_match_when_the_factor_names_one() {
        let breakdown = score(
            &[signal("product_detected", Some("authentik"), 0.9)],
            &iam(),
        );
        assert_eq!(breakdown.contributions[0].contribution, 0.0);
        assert_eq!(breakdown.total.get(), 5.0);
    }

    #[test]
    fn a_factor_counts_its_strongest_match_once() {
        let signals = [
            signal("saas_provider", Some("auth0"), 0.5),
            signal("saas_provider", Some("okta"), 0.9),
        ];
        let breakdown = score(&signals, &iam());
        assert!((breakdown.contributions[2].contribution - 9.0).abs() < 1e-9);
    }

    #[test]
    fn same_signals_score_differently_under_two_profiles() {
        let signals = [
            signal("product_detected", Some("keycloak"), 0.9),
            signal("saas_provider", Some("auth0"), 0.9),
        ];
        let other = Scoring {
            factors: vec![factor("saas_provider", None, 50.0)],
        };
        assert_ne!(score(&signals, &iam()).total, score(&signals, &other).total);
    }

    #[test]
    fn no_signal_scores_zero() {
        assert_eq!(score(&[], &iam()).total.get(), 0.0);
    }

    const KINDS: [&str; 4] = [
        "product_detected",
        "debt_marker",
        "saas_provider",
        "job_offer",
    ];

    fn signals_strategy() -> impl Strategy<Value = Vec<(usize, f64)>> {
        proptest::collection::vec((0..KINDS.len(), 0.0f64..=1.0), 0..8)
    }

    fn build(raw: &[(usize, f64)]) -> Vec<Signal> {
        raw.iter()
            .map(|(kind, strength)| {
                let value = (KINDS[*kind] == "product_detected").then_some("keycloak");
                let value = if KINDS[*kind] == "debt_marker" {
                    Some("legacy_auth_path")
                } else {
                    value
                };
                signal(KINDS[*kind], value, *strength)
            })
            .collect()
    }

    proptest! {
        #[test]
        fn adding_a_signal_never_lowers_a_positive_score(raw in signals_strategy(), extra in (0..KINDS.len(), 0.0f64..=1.0)) {
            let before = build(&raw);
            let mut after = before.clone();
            after.extend(build(&[extra]));
            prop_assert!(score(&after, &iam()).total.get() >= score(&before, &iam()).total.get());
        }

        #[test]
        fn raising_a_positive_weight_never_lowers_the_score(raw in signals_strategy(), index in 0usize..5, bump in 0.0f64..100.0) {
            let signals = build(&raw);
            let base = iam();
            let mut raised = base.clone();
            raised.factors[index].weight += bump;
            prop_assert!(score(&signals, &raised).total.get() >= score(&signals, &base).total.get());
        }

        #[test]
        fn total_is_the_sum_of_contributions(raw in signals_strategy()) {
            let breakdown = score(&build(&raw), &iam());
            let sum: f64 = breakdown.contributions.iter().map(|c| c.contribution).sum();
            prop_assert!((breakdown.total.get() - sum).abs() < 1e-9);
        }
    }
}
