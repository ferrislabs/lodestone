use chrono::DateTime;
use serde_json::{Value, json};

use super::*;
use crate::domain::profile::{ValidatedProfile, fixtures::MINIMAL_PROFILE};

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

fn definition() -> ProfileDefinition {
    let value: Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    ValidatedProfile::parse(value).unwrap().definition().clone()
}

fn profile_id() -> ProfileId {
    ProfileId(uuid::Uuid::nil())
}

fn at() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).unwrap()
}

fn observations(positives: usize, negatives: usize) -> Vec<Labelled> {
    let mut items = Vec::new();
    for index in 0..positives {
        items.push(Labelled {
            x: 0.5 + count(index) / count(positives.max(1)) / 2.0,
            validated: true,
        });
    }
    for index in 0..negatives {
        items.push(Labelled {
            x: count(index) / count(negatives.max(1)) / 2.0,
            validated: false,
        });
    }
    items
}

#[test]
fn the_good_probability_counts_half_of_the_to_check_one() {
    let probabilities = VerdictProbabilities::new(0.5, 0.3, 0.2);
    assert!(near(probabilities.good(), 0.65));
}

#[test]
fn the_transform_hits_the_calibrated_good_probability_and_keeps_to_check() {
    let probabilities = VerdictProbabilities::new(0.5, 0.3, 0.2);
    let moved = probabilities.with_good(0.4);
    assert!(near(moved.to_check, 0.3));
    assert!(near(moved.reliable, 0.25));
    assert!(near(moved.discarded, 0.45));
    assert!(near(moved.good(), 0.4));
    assert!(near(moved.sum(), 1.0));
}

#[test]
fn the_transform_is_the_identity_at_the_current_good_probability() {
    let probabilities = VerdictProbabilities::new(0.5, 0.3, 0.2);
    let same = probabilities.with_good(probabilities.good());
    assert!(near(same.reliable, 0.5));
    assert!(near(same.to_check, 0.3));
    assert!(near(same.discarded, 0.2));
}

#[test]
fn an_infeasible_target_is_clamped_and_the_sum_stays_one() {
    let probabilities = VerdictProbabilities::new(0.1, 0.8, 0.1);
    let low = probabilities.with_good(0.02);
    assert!(near(low.reliable, 0.0));
    assert!(near(low.to_check, 0.8));
    assert!(near(low.discarded, 0.2));
    assert!(near(low.sum(), 1.0));
    let high = probabilities.with_good(0.98);
    assert!(near(high.reliable, 0.2));
    assert!(near(high.discarded, 0.0));
    assert!(near(high.sum(), 1.0));
}

#[test]
fn the_transform_is_monotone_in_the_target() {
    let probabilities = VerdictProbabilities::new(0.3, 0.5, 0.2);
    let mut last = probabilities.with_good(0.0);
    for step in 1..=100 {
        let next = probabilities.with_good(f64::from(step) / 100.0);
        assert!(next.reliable >= last.reliable - 1e-12);
        assert!(next.discarded <= last.discarded + 1e-12);
        assert!(near(next.to_check, 0.5));
        assert!(near(next.sum(), 1.0));
        assert!(next.reliable >= 0.0 && next.discarded >= 0.0);
        last = next;
    }
}

#[test]
fn the_transform_normalises_a_loose_sum_first() {
    let loose = VerdictProbabilities::new(0.6, 0.3, 0.3);
    let moved = loose.with_good(0.5);
    assert!(near(moved.sum(), 1.0));
    assert!(near(moved.good(), 0.5));
}

#[test]
fn a_fit_needs_fifty_labelled_leads() {
    let few = fit(&observations(10, 39));
    assert_eq!(few.n, 49);
    assert!(few.map.is_none());
    assert!(few.brier_before.is_some());
    assert!(few.brier_after.is_none());
    let enough = fit(&observations(10, 40));
    assert_eq!(enough.n, 50);
    assert!(enough.map.is_some());
}

#[test]
fn a_fit_needs_ten_of_each_class() {
    assert!(fit(&observations(9, 60)).map.is_none());
    assert!(fit(&observations(60, 9)).map.is_none());
    assert!(fit(&observations(10, 60)).map.is_some());
    assert!(fit(&observations(60, 10)).map.is_some());
    assert!(!sufficient(60, 60));
    assert!(!sufficient(0, 0));
    assert!(sufficient(50, 25));
}

#[test]
fn the_brier_score_improves_in_sample_on_an_overconfident_agent() {
    let mut items = Vec::new();
    for index in 0..40 {
        items.push(Labelled {
            x: 0.9,
            validated: index % 2 == 0,
        });
        items.push(Labelled {
            x: 0.1,
            validated: index % 4 == 0,
        });
    }
    let fitted = fit(&items);
    let before = fitted.brier_before.unwrap();
    let after = fitted.brier_after.unwrap();
    assert!(after < before);
    assert!(near(
        before,
        0.5 * (0.5 * 0.1_f64.powi(2) + 0.5 * 0.9_f64.powi(2))
            + 0.5 * (0.25 * 0.9_f64.powi(2) + 0.75 * 0.1_f64.powi(2))
    ));
    let map = fitted.map.unwrap();
    assert!(near(map.predict(0.1), 0.25));
    assert!(near(map.predict(0.9), 0.5));
}

#[test]
fn non_finite_inputs_are_dropped_and_scores_are_clamped_to_the_unit_interval() {
    let mut items = observations(10, 40);
    items.push(Labelled {
        x: f64::NAN,
        validated: true,
    });
    items.push(Labelled {
        x: 3.0,
        validated: true,
    });
    let fitted = fit(&items);
    assert_eq!(fitted.n, 51);
    assert_eq!(fitted.positives, 11);
}

#[test]
fn the_reliability_curve_has_ten_bins_and_skips_empty_ones() {
    let items = [
        Labelled {
            x: 0.0,
            validated: false,
        },
        Labelled {
            x: 0.05,
            validated: true,
        },
        Labelled {
            x: 0.95,
            validated: true,
        },
        Labelled {
            x: 1.0,
            validated: true,
        },
    ];
    let bins = reliability(&items);
    assert_eq!(bins.len(), 2);
    assert_eq!(bins[0].n, 2);
    assert!(near(bins[0].predicted, 0.025));
    assert!(near(bins[0].observed, 0.5));
    assert_eq!(bins[1].n, 2);
    assert!(near(bins[1].predicted, 0.975));
    assert!(near(bins[1].observed, 1.0));
    assert!(bins.iter().all(|bin| bin.n <= 2));
}

fn stored(kind: CalibrationKind, agent: &AgentId, enabled: bool, y: f64) -> Calibration {
    Calibration {
        profile_id: profile_id(),
        kind,
        agent_id: agent.clone(),
        map: IsotonicMap::from_points(vec![Point { x: 0.0, y }]),
        n: 60,
        positives: 20,
        brier_before: None,
        brier_after: None,
        reliability: Vec::new(),
        fitted_at: at(),
        enabled,
    }
}

#[test]
fn only_enabled_calibrations_of_a_learning_profile_are_applied() {
    let agent = AgentId::new("a");
    let rows = vec![
        stored(CalibrationKind::Scoring, &agent, true, 0.4),
        stored(CalibrationKind::Qualification, &agent, false, 0.4),
    ];
    let applied = Calibrations::applied(rows.clone(), true);
    assert!(applied.scoring(&agent).is_some());
    assert!(applied.qualification(&agent).is_none());
    assert!(Calibrations::applied(rows, false).is_empty());
}

#[test]
fn the_status_follows_the_setting_then_the_fit() {
    let agent = AgentId::new("a");
    let active = stored(CalibrationKind::Scoring, &agent, true, 0.4);
    let inactive = stored(CalibrationKind::Scoring, &agent, false, 0.4);
    assert_eq!(active.status(true), CalibrationStatus::Active);
    assert_eq!(active.status(false), CalibrationStatus::Disabled);
    assert_eq!(inactive.status(true), CalibrationStatus::Insufficient);
}

#[test]
fn a_change_of_effect_is_detected_only_when_an_applied_map_differs() {
    let agent = AgentId::new("a");
    let base = vec![stored(CalibrationKind::Scoring, &agent, true, 0.4)];
    let refit = vec![Calibration {
        n: 90,
        fitted_at: at() + chrono::Duration::days(1),
        ..base[0].clone()
    }];
    assert!(!effect_changed(&base, &refit));
    let moved = vec![stored(CalibrationKind::Scoring, &agent, true, 0.5)];
    assert!(effect_changed(&base, &moved));
    let dropped = vec![stored(CalibrationKind::Scoring, &agent, false, 0.4)];
    assert!(effect_changed(&base, &dropped));
    assert!(effect_changed(&[], &base));
    assert!(!effect_changed(&[], &dropped));
    assert!(!effect_changed(&dropped, &[]));
}

fn scoring_row(
    agent: &AgentId,
    validated: bool,
    probabilities: (f64, f64, f64),
) -> LabelledScoring {
    LabelledScoring {
        agent_id: agent.clone(),
        validated,
        verdict: VerdictAnswer::from_probabilities(VerdictProbabilities::new(
            probabilities.0,
            probabilities.1,
            probabilities.2,
        ))
        .to_json(),
    }
}

#[test]
fn a_profile_fit_lists_every_agent_and_fits_only_the_sufficient_ones() {
    let definition = definition();
    let scoring_id = definition.scoring_agents[0].id.clone();
    let mut rows = Vec::new();
    for index in 0..60 {
        let validated = index % 3 != 0;
        let probabilities = if validated {
            (0.8, 0.1, 0.1)
        } else {
            (0.2, 0.2, 0.6)
        };
        rows.push(scoring_row(&scoring_id, validated, probabilities));
    }
    let fitted = fit_profile(profile_id(), &definition, &rows, &[], at());
    assert_eq!(
        fitted.len(),
        definition.scoring_agents.len() + definition.qualification_agents.len()
    );
    let scoring = fitted
        .iter()
        .find(|item| item.kind == CalibrationKind::Scoring && item.agent_id == scoring_id)
        .unwrap();
    assert_eq!(scoring.n, 60);
    assert_eq!(scoring.positives, 40);
    assert!(scoring.enabled);
    assert!(scoring.map.is_some());
    assert!(scoring.brier_after.unwrap() <= scoring.brier_before.unwrap());
    let qualification = fitted
        .iter()
        .find(|item| item.kind == CalibrationKind::Qualification)
        .unwrap();
    assert_eq!(qualification.n, 0);
    assert!(!qualification.enabled);
    assert!(qualification.map.is_none());
    assert!(qualification.brier_before.is_none());
}

#[test]
fn rows_of_other_agents_and_unreadable_verdicts_are_ignored() {
    let definition = definition();
    let scoring_id = definition.scoring_agents[0].id.clone();
    let rows = vec![
        scoring_row(&AgentId::new("elsewhere"), true, (0.8, 0.1, 0.1)),
        LabelledScoring {
            agent_id: scoring_id.clone(),
            validated: true,
            verdict: json!({"nope": 1}),
        },
        scoring_row(&scoring_id, false, (0.2, 0.2, 0.6)),
    ];
    let fitted = fit_profile(profile_id(), &definition, &rows, &[], at());
    let scoring = fitted
        .iter()
        .find(|item| item.kind == CalibrationKind::Scoring)
        .unwrap();
    assert_eq!(scoring.n, 1);
    assert_eq!(scoring.positives, 0);
}

fn answers_for(agent: &QualificationAgent, option_index: usize) -> Value {
    let mut object = serde_json::Map::new();
    for question in &agent.questions {
        let option = &question.options[option_index];
        let mut probabilities = serde_json::Map::new();
        for each in &question.options {
            probabilities.insert(
                each.value.clone(),
                json!(if each.value == option.value { 1.0 } else { 0.0 }),
            );
        }
        object.insert(
            question.id.clone(),
            json!({
                "type": "choice",
                "choice": option.value,
                "evidence": "",
                "confidence": 1.0,
                "probabilities": probabilities,
            }),
        );
    }
    Value::Object(object)
}

#[test]
fn qualification_scores_are_learned_from_stored_answers_without_the_floor() {
    let definition = definition();
    let agent = &definition.qualification_agents[0];
    let strong = agent.questions[0].options[0].score;
    let weak = agent
        .questions
        .iter()
        .flat_map(|question| question.options.iter())
        .map(|option| option.score)
        .fold(f64::MAX, f64::min);
    assert!(strong >= weak);
    let mut rows = Vec::new();
    for index in 0..60 {
        let validated = index % 2 == 0;
        rows.push(LabelledQualification {
            agent_id: agent.id.clone(),
            validated,
            answers: answers_for(agent, if validated { 0 } else { 1 }),
        });
    }
    let fitted = fit_profile(profile_id(), &definition, &[], &rows, at());
    let found = fitted
        .iter()
        .find(|item| item.kind == CalibrationKind::Qualification && item.agent_id == agent.id)
        .unwrap();
    assert_eq!(found.n, 60);
    assert_eq!(found.positives, 30);
    let answers = stored_answers(agent, &rows[0].answers).unwrap();
    let raw = raw_agent_score(agent, &answers);
    assert!(near(raw, agent_score(agent, &answers, 0.0)));
    assert!(found.enabled);
    let calibrations = Calibrations::applied(fitted.clone(), true);
    let used = opinion_score(agent, &answers, 0.05, &calibrations);
    assert!(near(used, found.predict(raw).unwrap()));
    let none = opinion_score(agent, &answers, 0.05, &Calibrations::none());
    assert!(near(none, agent_score(agent, &answers, 0.05)));
}

#[test]
fn the_qualification_opinion_uses_the_learned_map_instead_of_the_floor() {
    let definition = definition();
    let agent = &definition.qualification_agents[0];
    let answers = stored_answers(agent, &answers_for(agent, 0)).unwrap();
    let learned = Calibration {
        kind: CalibrationKind::Qualification,
        ..stored(CalibrationKind::Qualification, &agent.id, true, 0.37)
    };
    let calibrations = Calibrations::applied(vec![learned], true);
    assert!(near(
        opinion_score(agent, &answers, 0.05, &calibrations),
        0.37
    ));
    let off = Calibrations::applied(
        vec![stored(
            CalibrationKind::Qualification,
            &agent.id,
            true,
            0.37,
        )],
        false,
    );
    assert!(near(
        opinion_score(agent, &answers, 0.05, &off),
        agent_score(agent, &answers, 0.05)
    ));
}

#[test]
fn the_report_lists_every_agent_with_its_state() {
    let definition = definition();
    let scoring_id = definition.scoring_agents[0].id.clone();
    let stored_rows = vec![stored(CalibrationKind::Scoring, &scoring_id, true, 0.4)];
    let report = profile_report(profile_id(), "IAM", &definition, &stored_rows);
    assert!(report.learned);
    assert_eq!(report.name, "IAM");
    assert_eq!(
        report.agents.len(),
        definition.scoring_agents.len() + definition.qualification_agents.len()
    );
    assert_eq!(report.agents[0].kind, CalibrationKind::Scoring);
    assert_eq!(report.agents[0].status, CalibrationStatus::Active);
    assert_eq!(report.agents[0].n, 60);
    let last = report.agents.last().unwrap();
    assert_eq!(last.kind, CalibrationKind::Qualification);
    assert_eq!(last.status, CalibrationStatus::Insufficient);
    assert_eq!(last.n, 0);
    assert!(last.fitted_at.is_none());
}

#[test]
fn a_profile_that_does_not_learn_reports_every_agent_as_disabled() {
    let mut definition = definition();
    definition.calibration.learned = false;
    let scoring_id = definition.scoring_agents[0].id.clone();
    let stored_rows = vec![stored(CalibrationKind::Scoring, &scoring_id, true, 0.4)];
    let report = profile_report(profile_id(), "IAM", &definition, &stored_rows);
    assert!(!report.learned);
    assert!(
        report
            .agents
            .iter()
            .all(|agent| agent.status == CalibrationStatus::Disabled)
    );
    assert_eq!(report.agents[0].n, 60);
}

#[test]
fn ten_equal_width_bins_cover_the_unit_interval() {
    let items: Vec<Labelled> = (0..10)
        .map(|bin| Labelled {
            x: (count(bin) + 0.5) / 10.0,
            validated: bin >= 5,
        })
        .collect();
    let bins = reliability(&items);
    assert_eq!(bins.len(), RELIABILITY_BINS);
    assert!(bins.iter().all(|bin| bin.n == 1));
    assert!(
        bins.windows(2)
            .all(|pair| pair[0].predicted < pair[1].predicted)
    );
    assert!(near(bins[0].observed, 0.0));
    assert!(near(bins[9].observed, 1.0));
}
