use chrono::DateTime;
use serde_json::{Value, json};

use super::*;
use crate::domain::{
    llm::RunProvenance,
    profile::{ValidatedProfile, fixtures::MINIMAL_PROFILE},
    qualification::{Exclusion, VerdictReason},
};

fn agent() -> ScoringAgent {
    let value: Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    ValidatedProfile::parse(value)
        .unwrap()
        .definition()
        .scoring_agents[0]
        .clone()
}

const QUOTE: &str = "sso.acme.com expose Keycloak";

fn documents() -> Vec<UntrustedDocument<'static>> {
    vec![UntrustedDocument {
        source: "https://sso.acme.com/",
        content: "sso.acme.com expose Keycloak (sonde HTTP, version 21.1)",
    }]
}

fn criterion(choice: &str, evidence: &str) -> Value {
    json!({
        "type": "choice",
        "choice": choice,
        "evidence": evidence,
        "confidence": 0.8,
        "probabilities": {"forte": 0.0, "moyenne": 0.0, "faible": 0.0, "inconnu": 0.0, choice: 1.0}
    })
}

fn criterion_with(options: [&str; 4], choice: &str, evidence: &str) -> Value {
    let mut probabilities = serde_json::Map::new();
    for option in options {
        probabilities.insert(
            option.to_owned(),
            json!(if option == choice { 1.0 } else { 0.0 }),
        );
    }
    json!({
        "type": "choice",
        "choice": choice,
        "evidence": evidence,
        "confidence": 0.8,
        "probabilities": probabilities
    })
}

fn raw() -> Value {
    json!({
        "criteria": {
            "adequation_offre": criterion("forte", QUOTE),
            "solidite_preuves": criterion_with(["solide", "partielle", "fragile", "inconnu"], "inconnu", ""),
            "priorite_commerciale": criterion_with(["haute", "normale", "basse", "inconnu"], "inconnu", "")
        },
        "score": 72,
        "verdict": {
            "choice": "a_verifier",
            "confidence": 0.6,
            "probabilities": {"fiable": 0.3, "a_verifier": 0.6, "ecarte": 0.1}
        },
        "reasons": ["Un seul signal.", "Entreprise ciblée."]
    })
}

fn valid(mutate: impl FnOnce(&mut Value)) -> ScoringOutput {
    let mut value = raw();
    mutate(&mut value);
    validate_scoring(&agent(), &value, &documents()).unwrap()
}

fn violations(mutate: impl FnOnce(&mut Value)) -> Vec<ScoringViolation> {
    let mut value = raw();
    mutate(&mut value);
    validate_scoring(&agent(), &value, &documents())
        .unwrap_err()
        .0
}

fn near(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

#[test]
fn a_conforming_answer_is_accepted_with_its_criteria_grounded() {
    let output = valid(|_| {});
    assert_eq!(output.score, 72);
    assert_eq!(output.verdict.choice, Verdict::ToCheck);
    assert_eq!(output.reasons.len(), 2);
    let first = &output.criteria.0[0];
    assert_eq!(first.choice, "forte");
    assert!(first.grounded);
    assert_eq!(
        first.evidence_source.as_deref(),
        Some("https://sso.acme.com/")
    );
}

#[test]
fn the_score_is_an_integer_from_zero_to_one_hundred() {
    assert_eq!(valid(|v| v["score"] = json!(0)).score, 0);
    assert_eq!(valid(|v| v["score"] = json!(100)).score, 100);
    assert_eq!(valid(|v| v["score"] = json!(72.0)).score, 72);
    for bad in [json!(-1), json!(101), json!(72.5), json!("72"), json!(null)] {
        assert!(
            violations(|v| v["score"] = bad.clone()).contains(&ScoringViolation::BadScore),
            "{bad}"
        );
    }
    assert!(
        violations(|v| {
            v.as_object_mut().unwrap().remove("score");
        })
        .contains(&ScoringViolation::BadScore)
    );
}

#[test]
fn the_verdict_choice_must_be_one_of_the_three_values() {
    let found = violations(|v| v["verdict"]["choice"] = json!("peut_etre"));
    assert!(matches!(
        found[0],
        ScoringViolation::BadVerdictChoice { .. }
    ));
    let found = violations(|v| v["verdict"]["choice"] = json!(null));
    assert!(matches!(
        found[0],
        ScoringViolation::BadVerdictChoice { .. }
    ));
}

#[test]
fn the_verdict_confidence_is_a_probability() {
    assert!(
        violations(|v| v["verdict"]["confidence"] = json!(1.5))
            .contains(&ScoringViolation::BadVerdictConfidence)
    );
}

#[test]
fn the_verdict_probabilities_carry_exactly_the_three_keys() {
    for bad in [
        json!({"fiable": 0.5, "a_verifier": 0.5}),
        json!({"fiable": 0.3, "a_verifier": 0.6, "ecarte": 0.1, "autre": 0.0}),
        json!([0.3, 0.6, 0.1]),
    ] {
        assert!(
            violations(|v| v["verdict"]["probabilities"] = bad.clone())
                .contains(&ScoringViolation::VerdictProbabilityKeys)
        );
    }
    let found = violations(|v| {
        v["verdict"]["probabilities"] = json!({"fiable": 2, "a_verifier": 0, "ecarte": 0})
    });
    assert!(matches!(
        found[0],
        ScoringViolation::BadVerdictProbability { .. }
    ));
}

#[test]
fn the_verdict_probabilities_sum_to_one_within_tolerance_and_are_renormalised() {
    let found = violations(|v| {
        v["verdict"]["probabilities"] = json!({"fiable": 0.3, "a_verifier": 0.4, "ecarte": 0.1})
    });
    assert!(matches!(
        found[0],
        ScoringViolation::VerdictProbabilitySum { .. }
    ));
    let output = valid(|v| {
        v["verdict"]["probabilities"] = json!({"fiable": 0.31, "a_verifier": 0.62, "ecarte": 0.1})
    });
    let probabilities = output.verdict.probabilities;
    assert!(near(probabilities.sum(), 1.0));
    assert!(near(probabilities.to_check, 0.62 / 1.03));
}

#[test]
fn the_verdict_follows_the_probabilities_and_keeps_the_declared_choice() {
    let output = valid(|v| v["verdict"]["choice"] = json!("fiable"));
    assert_eq!(output.verdict.choice, Verdict::ToCheck);
    assert_eq!(output.verdict.declared_choice, Some(Verdict::Reliable));
    assert!(near(output.verdict.confidence, 0.6));
    let consistent = valid(|_| {});
    assert_eq!(consistent.verdict.declared_choice, None);
}

#[test]
fn reasons_are_one_to_five_short_non_empty_strings() {
    for bad in [
        json!([]),
        json!(["a", "b", "c", "d", "e", "f"]),
        json!([""]),
        json!(["   "]),
        json!([1]),
        json!("raison"),
    ] {
        assert!(
            violations(|v| v["reasons"] = bad.clone()).contains(&ScoringViolation::BadReasons),
            "{bad}"
        );
    }
    let found = violations(|v| v["reasons"] = json!(["x".repeat(201)]));
    assert_eq!(
        found,
        vec![ScoringViolation::ReasonTooLong {
            index: 0,
            length: 201
        }]
    );
    let output = valid(|v| v["reasons"] = json!(["x".repeat(200), "  y  ", "z", "t", "u"]));
    assert_eq!(output.reasons[1], "y");
    assert_eq!(output.reasons.len(), 5);
}

#[test]
fn the_criteria_object_is_required_and_complete() {
    assert_eq!(
        violations(|v| {
            v.as_object_mut().unwrap().remove("criteria");
        }),
        vec![ScoringViolation::MissingCriteria]
    );
    let found = violations(|v| {
        v["criteria"]
            .as_object_mut()
            .unwrap()
            .remove("solidite_preuves");
    });
    assert_eq!(
        found,
        vec![ScoringViolation::Criterion(Violation::MissingQuestion {
            question: "solidite_preuves".to_owned()
        })]
    );
}

#[test]
fn a_criterion_choice_must_be_one_of_its_options() {
    let found = violations(|v| v["criteria"]["adequation_offre"]["choice"] = json!("enorme"));
    assert!(matches!(
        found[0],
        ScoringViolation::Criterion(Violation::UnknownChoice { .. })
    ));
}

#[test]
fn a_criterion_without_a_verifiable_quote_is_downgraded_to_unknown() {
    let output = valid(|v| {
        v["criteria"]["adequation_offre"] = criterion("forte", "citation inventée");
    });
    let first = &output.criteria.0[0];
    assert_eq!(first.choice, "inconnu");
    assert!(!first.grounded);
    assert_eq!(first.original_choice.as_deref(), Some("forte"));
    assert!(first.evidence_source.is_none());
    assert!(near(
        first
            .probabilities
            .iter()
            .find(|(option, _)| option == "inconnu")
            .unwrap()
            .1,
        1.0
    ));
}

#[test]
fn an_unknown_criterion_needs_no_quote() {
    let output = valid(|_| {});
    let second = &output.criteria.0[1];
    assert_eq!(second.choice, "inconnu");
    assert!(second.grounded);
}

#[test]
fn a_non_object_answer_is_rejected() {
    let found = validate_scoring(&agent(), &json!([1]), &documents()).unwrap_err();
    assert_eq!(found.0, vec![ScoringViolation::NotAnObject]);
}

#[test]
fn every_problem_is_reported_at_once() {
    let found = violations(|v| {
        v["score"] = json!(200);
        v["reasons"] = json!([]);
        v["verdict"] = json!(null);
    });
    assert_eq!(found.len(), 3);
    let text = ScoringViolations(found).to_string();
    assert!(text.contains("score"));
    assert!(text.contains("reasons"));
    assert!(text.contains("verdict"));
}

#[test]
fn the_output_json_keeps_the_contract_shape() {
    let json = valid(|_| {}).to_json();
    assert_eq!(json["score"], 72);
    assert_eq!(json["verdict"]["choice"], "a_verifier");
    assert_eq!(json["reasons"][0], "Un seul signal.");
    assert_eq!(json["criteria"]["adequation_offre"]["choice"], "forte");
    assert_eq!(json["criteria"]["adequation_offre"]["grounded"], true);
}

#[test]
fn a_verdict_answer_survives_its_stored_form() {
    let answer = valid(|v| v["verdict"]["choice"] = json!("fiable")).verdict;
    let restored = VerdictAnswer::from_json(&answer.to_json()).unwrap();
    assert_eq!(restored, answer);
}

fn sample(score: u8, probabilities: (f64, f64, f64), choice: &str, reason: &str) -> ScoringOutput {
    ScoringOutput {
        criteria: valid(|v| {
            v["criteria"]["adequation_offre"] = criterion(choice, QUOTE);
        })
        .criteria,
        score,
        verdict: VerdictAnswer::from_probabilities(VerdictProbabilities::new(
            probabilities.0,
            probabilities.1,
            probabilities.2,
        )),
        reasons: vec![reason.to_owned()],
    }
}

#[test]
fn samples_average_the_score_and_the_verdict_probabilities() {
    let samples = [
        sample(70, (0.2, 0.7, 0.1), "forte", "a"),
        sample(80, (0.4, 0.5, 0.1), "forte", "b"),
        sample(75, (0.3, 0.6, 0.1), "moyenne", "c"),
    ];
    let voted = vote_scoring(&agent(), &samples).unwrap();
    assert_eq!(voted.score, 75);
    assert!(near(voted.verdict.probabilities.reliable, 0.3));
    assert!(near(voted.verdict.probabilities.to_check, 0.6));
    assert!(near(voted.verdict.probabilities.discarded, 0.1));
    assert_eq!(voted.verdict.choice, Verdict::ToCheck);
    assert!(near(voted.verdict.confidence, 0.6));
}

#[test]
fn samples_vote_the_criteria_with_agreement() {
    let samples = [
        sample(70, (0.2, 0.7, 0.1), "forte", "a"),
        sample(80, (0.4, 0.5, 0.1), "forte", "b"),
        sample(75, (0.3, 0.6, 0.1), "moyenne", "c"),
    ];
    let voted = vote_scoring(&agent(), &samples).unwrap();
    let first = &voted.criteria.0[0];
    assert_eq!(first.choice, "forte");
    assert!(near(first.agreement.unwrap(), 2.0 / 3.0));
    assert_eq!(voted.reasons, vec!["a".to_owned()]);
}

#[test]
fn the_score_average_rounds_to_the_nearest_integer() {
    let samples = [
        sample(70, (0.2, 0.7, 0.1), "forte", "a"),
        sample(71, (0.2, 0.7, 0.1), "forte", "b"),
    ];
    assert_eq!(vote_scoring(&agent(), &samples).unwrap().score, 71);
}

#[test]
fn the_reasons_come_from_a_sample_that_agrees_with_the_final_verdict() {
    let samples = [
        sample(20, (0.0, 0.1, 0.9), "faible", "contre"),
        sample(60, (0.1, 0.8, 0.1), "forte", "pour a"),
        sample(62, (0.1, 0.8, 0.1), "forte", "pour b"),
        sample(64, (0.1, 0.8, 0.1), "forte", "pour c"),
    ];
    let voted = vote_scoring(&agent(), &samples).unwrap();
    assert_eq!(voted.verdict.choice, Verdict::ToCheck);
    assert_eq!(voted.reasons, vec!["pour a".to_owned()]);
}

#[test]
fn a_single_sample_is_kept_with_full_agreement() {
    let only = sample(70, (0.2, 0.7, 0.1), "forte", "a");
    let voted = vote_scoring(&agent(), std::slice::from_ref(&only)).unwrap();
    assert_eq!(voted.score, 70);
    assert_eq!(voted.verdict, only.verdict);
    assert!(near(voted.criteria.0[0].agreement.unwrap(), 1.0));
    assert!(vote_scoring(&agent(), &[]).is_none());
}

#[test]
fn the_raw_record_keeps_valid_and_invalid_samples() {
    let valid = sample(70, (0.2, 0.7, 0.1), "forte", "a");
    let kept = scoring_sample_record(0, Ok(&valid));
    assert_eq!(kept["valid"], true);
    assert_eq!(kept["score"], 70);
    let dropped = scoring_sample_record(2, Err("contrat"));
    assert_eq!(dropped["valid"], false);
    assert_eq!(dropped["error"], "contrat");
}

fn agent_with(id: &str, weight: u8) -> ScoringAgent {
    ScoringAgent {
        id: AgentId::new(id),
        weight,
        ..agent()
    }
}

fn result_for(agent: &ScoringAgent, score: u8, probabilities: (f64, f64, f64)) -> ScoringResult {
    ScoringResult {
        id: ScoringResultId(uuid::Uuid::nil()),
        lead_id: crate::domain::lead::LeadId(uuid::Uuid::nil()),
        profile_id: crate::domain::profile::ProfileId(uuid::Uuid::nil()),
        profile_revision: RevisionNumber::FIRST,
        agent_id: agent.id.clone(),
        provenance: RunProvenance::legacy("m".to_owned(), "h".to_owned()),
        status: ResultStatus::Valid,
        criteria: json!({}),
        score: Some(score),
        verdict: Some(VerdictAnswer::from_probabilities(
            VerdictProbabilities::new(probabilities.0, probabilities.1, probabilities.2),
        )),
        reasons: vec!["r".to_owned()],
        error: None,
        raw_samples: json!([]),
        created_at: DateTime::from_timestamp(0, 0).unwrap(),
    }
}

fn invalid_for(agent: &ScoringAgent) -> ScoringResult {
    ScoringResult {
        status: ResultStatus::Invalid,
        score: None,
        verdict: None,
        reasons: Vec::new(),
        error: Some("contrat".to_owned()),
        ..result_for(agent, 0, (0.0, 0.0, 0.0))
    }
}

fn exclusion() -> Exclusion {
    Exclusion {
        agent_id: AgentId::new("pertinence"),
        agent_name: "Pertinence".to_owned(),
        question_id: "q".to_owned(),
        option: "concurrent".to_owned(),
        label: "Concurrent".to_owned(),
        probability: 0.9,
        evidence: "Nous sommes concurrents".to_owned(),
        evidence_source: None,
    }
}

#[test]
fn the_lead_score_is_the_weighted_mean_of_the_agent_scores() {
    let heavy = agent_with("lourd", 9);
    let light = agent_with("leger", 1);
    let results = [
        result_for(&heavy, 80, (0.1, 0.8, 0.1)),
        result_for(&light, 20, (0.1, 0.8, 0.1)),
    ];
    let decision = decide(&[heavy, light], &results, Vec::new());
    assert_eq!(decision.score, Some(74));
    assert_eq!(decision.contributors, 2);
}

#[test]
fn the_verdict_is_the_argmax_of_the_weighted_mean_of_the_probabilities() {
    let heavy = agent_with("lourd", 3);
    let light = agent_with("leger", 1);
    let results = [
        result_for(&heavy, 80, (0.7, 0.2, 0.1)),
        result_for(&light, 20, (0.1, 0.2, 0.7)),
    ];
    let decision = decide(&[heavy, light], &results, Vec::new());
    let mean = decision.probabilities.unwrap();
    assert!(near(mean.reliable, 0.55));
    assert!(near(mean.to_check, 0.2));
    assert!(near(mean.discarded, 0.25));
    assert_eq!(decision.verdict, Some(Verdict::Reliable));
    assert_eq!(decision.confidence, Some(55.0));
    assert_eq!(decision.reason, None);
}

#[test]
fn ties_resolve_prudently_to_check_then_discarded_then_reliable() {
    let one = agent_with("un", 5);
    let tie = |p: (f64, f64, f64)| {
        decide(
            std::slice::from_ref(&one),
            &[result_for(&one, 50, p)],
            Vec::new(),
        )
        .verdict
    };
    assert_eq!(tie((0.5, 0.5, 0.0)), Some(Verdict::ToCheck));
    assert_eq!(tie((0.5, 0.0, 0.5)), Some(Verdict::Discarded));
    assert_eq!(tie((0.0, 0.5, 0.5)), Some(Verdict::ToCheck));
    assert_eq!(
        tie((1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0)),
        Some(Verdict::ToCheck)
    );
    assert_eq!(tie((0.6, 0.3, 0.1)), Some(Verdict::Reliable));
}

#[test]
fn an_exclusion_forces_discarded_whatever_the_scoring_agents_say() {
    let one = agent_with("un", 5);
    let results = [result_for(&one, 90, (0.95, 0.04, 0.01))];
    let decision = decide(&[one], &results, vec![exclusion()]);
    assert_eq!(decision.verdict, Some(Verdict::Discarded));
    assert_eq!(decision.reason, Some(VerdictReason::Excluded));
    assert_eq!(decision.reason_text(), Some("excluded"));
    assert_eq!(decision.exclusions.len(), 1);
    assert_eq!(decision.score, Some(90));
    assert_eq!(decision.confidence, Some(100.0));
}

#[test]
fn an_exclusion_decides_even_without_any_valid_scoring_result() {
    let one = agent_with("un", 5);
    let decision = decide(&[one], &[], vec![exclusion()]);
    assert_eq!(decision.verdict, Some(Verdict::Discarded));
    assert_eq!(decision.reason, Some(VerdictReason::Excluded));
    assert_eq!(decision.score, None);
}

#[test]
fn no_valid_scoring_result_leaves_the_verdict_pending() {
    let one = agent_with("un", 5);
    for results in [Vec::new(), vec![invalid_for(&one)]] {
        let decision = decide(std::slice::from_ref(&one), &results, Vec::new());
        assert_eq!(decision.verdict, None);
        assert_eq!(decision.score, None);
        assert_eq!(decision.confidence, None);
        assert_eq!(decision.reason, Some(VerdictReason::AwaitingScoring));
        assert_eq!(decision.reason_text(), Some("awaiting_scoring"));
        assert_eq!(decision.contributors, 0);
    }
}

#[test]
fn an_invalid_agent_is_left_out_of_the_mean() {
    let good = agent_with("bon", 5);
    let bad = agent_with("mauvais", 9);
    let results = [result_for(&good, 40, (0.1, 0.8, 0.1)), invalid_for(&bad)];
    let decision = decide(&[good, bad], &results, Vec::new());
    assert_eq!(decision.score, Some(40));
    assert_eq!(decision.contributors, 1);
}

#[test]
fn a_disabled_agent_does_not_count() {
    let on = agent_with("actif", 5);
    let off = ScoringAgent {
        enabled: false,
        ..agent_with("coupe", 10)
    };
    let results = [
        result_for(&on, 60, (0.1, 0.8, 0.1)),
        result_for(&off, 0, (0.0, 0.0, 1.0)),
    ];
    let decision = decide(&[on, off], &results, Vec::new());
    assert_eq!(decision.score, Some(60));
    assert_eq!(decision.verdict, Some(Verdict::ToCheck));
}

#[test]
fn calibration_is_the_identity_until_the_calibration_issue_lands() {
    let probabilities = VerdictProbabilities::new(0.7, 0.2, 0.1);
    assert_eq!(calibrate_verdict(probabilities, &agent()), probabilities);
}

#[test]
fn the_rounding_of_scores_is_half_up_and_bounded() {
    assert_eq!(round_score(72.5), 73);
    assert_eq!(round_score(72.49), 72);
    assert_eq!(round_score(-3.0), 0);
    assert_eq!(round_score(140.0), 100);
}
