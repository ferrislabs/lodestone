use chrono::{DateTime, Utc};

use crate::domain::{
    facts::IndependentSignal,
    lead::Score,
    profile::definition::EvidenceSettings,
    qualification::confidence::{age_days, decay},
};

pub const STRONGEST_SIGNALS: usize = 3;

pub fn signal_strength(
    signals: &[IndependentSignal],
    settings: &EvidenceSettings,
    now: DateTime<Utc>,
) -> f64 {
    let mut weighted: Vec<f64> = signals
        .iter()
        .map(|signal| {
            signal.strength.get()
                * decay(
                    age_days(signal.observed_at, now),
                    f64::from(settings.half_life_days_for(signal.kind.as_str())),
                )
        })
        .collect();
    weighted.sort_by(|left, right| right.total_cmp(left));
    let strongest = &weighted[..weighted.len().min(STRONGEST_SIGNALS)];
    if strongest.is_empty() {
        return 0.0;
    }
    let count = f64::from(u32::try_from(strongest.len()).unwrap_or(u32::MAX));
    (strongest.iter().sum::<f64>() / count).clamp(0.0, 1.0)
}

pub fn strength_score(strength: f64) -> Score {
    Score::new((strength * 1000.0).round() / 10.0).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use proptest::prelude::*;

    use super::*;
    use crate::domain::values::{SignalKind, UnitInterval};

    fn now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).unwrap()
    }

    fn signal(kind: &str, strength: f64, age_days: i64) -> IndependentSignal {
        IndependentSignal {
            kind: SignalKind::parse(kind).unwrap(),
            strength: UnitInterval::new(strength).unwrap(),
            observed_at: now() - chrono::Duration::days(age_days),
        }
    }

    fn strength_of(signals: &[IndependentSignal]) -> f64 {
        signal_strength(signals, &EvidenceSettings::default(), now())
    }

    #[test]
    fn no_signal_has_no_strength() {
        assert_eq!(strength_of(&[]), 0.0);
        assert_eq!(strength_score(0.0).get(), 0.0);
    }

    #[test]
    fn fewer_than_three_signals_are_averaged_among_themselves() {
        let one = [signal("saas_provider", 0.8, 0)];
        assert!((strength_of(&one) - 0.8).abs() < 1e-9);
        let two = [signal("saas_provider", 0.8, 0), signal("job_offer", 0.4, 0)];
        assert!((strength_of(&two) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn only_the_three_strongest_signals_count() {
        let signals = [
            signal("saas_provider", 0.9, 0),
            signal("saas_provider", 0.6, 0),
            signal("saas_provider", 0.3, 0),
            signal("saas_provider", 0.1, 0),
            signal("saas_provider", 0.05, 0),
        ];
        assert!((strength_of(&signals) - 0.6).abs() < 1e-9);
    }

    #[test]
    fn an_old_signal_weighs_by_its_half_life() {
        let aged = [signal("saas_provider", 1.0, 365)];
        assert!((strength_of(&aged) - 0.5).abs() < 1e-9);
        let job = [signal("job_offer", 1.0, 90)];
        assert!((strength_of(&job) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn decay_can_reorder_the_strongest_signals() {
        let signals = [
            signal("job_offer", 1.0, 360),
            signal("saas_provider", 0.5, 0),
            signal("saas_provider", 0.4, 0),
            signal("saas_provider", 0.3, 0),
        ];
        let expected = (0.5 + 0.4 + 0.3) / 3.0;
        assert!((strength_of(&signals) - expected).abs() < 1e-9);
    }

    #[test]
    fn the_score_is_the_strength_in_percent_with_one_decimal() {
        assert_eq!(strength_score(0.6).get(), 60.0);
        assert_eq!(strength_score(0.123_456).get(), 12.3);
        assert_eq!(strength_score(1.0).get(), 100.0);
    }

    proptest! {
        #[test]
        fn strength_stays_within_zero_and_one(
            raw in proptest::collection::vec((0.0f64..=1.0, 0i64..2000), 0..10)
        ) {
            let signals: Vec<IndependentSignal> = raw
                .iter()
                .map(|(strength, age)| signal("saas_provider", *strength, *age))
                .collect();
            let strength = strength_of(&signals);
            prop_assert!((0.0..=1.0).contains(&strength));
        }

        #[test]
        fn adding_a_signal_never_lowers_the_best_single_signal_bound(
            raw in proptest::collection::vec(0.0f64..=1.0, 1..10)
        ) {
            let signals: Vec<IndependentSignal> = raw
                .iter()
                .map(|strength| signal("saas_provider", *strength, 0))
                .collect();
            let best = raw.iter().copied().fold(0.0, f64::max);
            prop_assert!(strength_of(&signals) <= best + 1e-9);
        }
    }
}
