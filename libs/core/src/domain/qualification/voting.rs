use serde_json::{Value, json};

use super::{Answer, ValidAnswers, ValidOutput};
use crate::domain::profile::definition::{QualificationAgent, Question};

pub const VOTE_SHARE_WEIGHT: f64 = 0.5;
const EPSILON: f64 = 1e-9;

fn count_of(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn probability_of(answer: &Answer, option: &str) -> f64 {
    answer
        .probabilities
        .iter()
        .find(|(value, _)| value == option)
        .map_or(0.0, |(_, probability)| *probability)
}

fn vote_question(question: &Question, samples: &[&ValidAnswers]) -> Option<Answer> {
    let picks: Vec<&Answer> = samples
        .iter()
        .filter_map(|sample| {
            sample
                .0
                .iter()
                .find(|answer| answer.question_id == question.id)
        })
        .collect();
    if picks.is_empty() {
        return None;
    }
    let total = count_of(picks.len());
    let votes = |option: &str| {
        picks
            .iter()
            .filter(|answer| answer.choice == option)
            .count()
    };
    let mean = |option: &str| {
        picks
            .iter()
            .map(|answer| probability_of(answer, option))
            .sum::<f64>()
            / total
    };
    let mut winner: Option<(&str, usize, f64)> = None;
    for option in &question.options {
        let count = votes(&option.value);
        if count == 0 {
            continue;
        }
        let average = mean(&option.value);
        let better = winner.is_none_or(|(_, best, best_mean)| {
            count > best || (count == best && average > best_mean + EPSILON)
        });
        if better {
            winner = Some((&option.value, count, average));
        }
    }
    let (choice, count, _) = winner?;
    let blended: Vec<(String, f64)> = question
        .options
        .iter()
        .map(|option| {
            let share = count_of(votes(&option.value)) / total;
            (
                option.value.clone(),
                VOTE_SHARE_WEIGHT * share + (1.0 - VOTE_SHARE_WEIGHT) * mean(&option.value),
            )
        })
        .collect();
    let sum: f64 = blended.iter().map(|(_, probability)| probability).sum();
    let probabilities = if sum > EPSILON {
        blended
            .into_iter()
            .map(|(option, probability)| (option, probability / sum))
            .collect()
    } else {
        blended
    };
    let supporters: Vec<&Answer> = picks
        .iter()
        .copied()
        .filter(|answer| answer.choice == choice)
        .collect();
    let representative = supporters
        .iter()
        .copied()
        .find(|answer| answer.grounded)
        .or_else(|| supporters.first().copied())?;
    let confidence = supporters
        .iter()
        .map(|answer| answer.confidence)
        .sum::<f64>()
        / count_of(supporters.len());
    Some(Answer {
        question_id: question.id.clone(),
        choice: choice.to_owned(),
        confidence,
        probabilities,
        evidence: representative.evidence.clone(),
        evidence_source: representative.evidence_source.clone(),
        grounded: supporters.iter().any(|answer| answer.grounded),
        original_choice: representative.original_choice.clone(),
        agreement: Some(count_of(count) / total),
    })
}

fn agreeing_axes(sample: &ValidAnswers, voted: &[Answer]) -> usize {
    voted
        .iter()
        .filter(|answer| {
            sample
                .0
                .iter()
                .any(|own| own.question_id == answer.question_id && own.choice == answer.choice)
        })
        .count()
}

pub fn most_agreeing(samples: &[&ValidAnswers], voted: &ValidAnswers) -> usize {
    let mut best = 0;
    let mut best_axes = 0;
    for (index, sample) in samples.iter().enumerate() {
        let axes = agreeing_axes(sample, &voted.0);
        if index == 0 || axes > best_axes {
            best = index;
            best_axes = axes;
        }
    }
    best
}

pub fn vote_answers(questions: &[Question], samples: &[&ValidAnswers]) -> Option<ValidAnswers> {
    let first = samples.first()?;
    if samples.len() == 1 {
        return Some(ValidAnswers(
            first
                .0
                .iter()
                .map(|answer| Answer {
                    agreement: Some(1.0),
                    ..answer.clone()
                })
                .collect(),
        ));
    }
    Some(ValidAnswers(
        questions
            .iter()
            .filter_map(|question| vote_question(question, samples))
            .collect(),
    ))
}

pub fn vote(agent: &QualificationAgent, samples: &[ValidOutput]) -> Option<ValidOutput> {
    let answers: Vec<&ValidAnswers> = samples.iter().map(|sample| &sample.answers).collect();
    let voted = vote_answers(&agent.questions, &answers)?;
    let best = most_agreeing(&answers, &voted);
    Some(ValidOutput {
        summary: samples[best].summary.clone(),
        answers: voted,
    })
}

pub fn sample_record(index: u32, outcome: Result<&ValidOutput, &str>) -> Value {
    match outcome {
        Ok(output) => json!({
            "index": index,
            "valid": true,
            "summary": output.summary,
            "answers": output.answers.to_json(),
        }),
        Err(error) => json!({
            "index": index,
            "valid": false,
            "error": error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profile::definition::{AgentId, QuestionOption};

    fn option(value: &str) -> QuestionOption {
        QuestionOption {
            value: value.to_owned(),
            label: value.to_uppercase(),
            score: 0.5,
            is_unknown: value == "inconnu",
            excludes: false,
        }
    }

    fn question(id: &str) -> Question {
        Question {
            id: id.to_owned(),
            wording: "?".to_owned(),
            weight: 5,
            options: ["forte", "moyenne", "faible", "inconnu"]
                .map(option)
                .to_vec(),
        }
    }

    fn agent(ids: &[&str]) -> QualificationAgent {
        QualificationAgent {
            id: AgentId::new("pertinence"),
            name: "Pertinence".to_owned(),
            description: "La pertinence".to_owned(),
            enabled: true,
            provider_id: None,
            system_prompt: "prompt".to_owned(),
            weight: 5,
            exclusion_threshold: 0.6,
            samples: 3,
            sampling_temperature: 0.7,
            learned_calibration: true,
            questions: ids.iter().map(|id| question(id)).collect(),
        }
    }

    fn answer(id: &str, choice: &str, probabilities: [f64; 4], quote: &str) -> Answer {
        let options = ["forte", "moyenne", "faible", "inconnu"];
        Answer {
            question_id: id.to_owned(),
            choice: choice.to_owned(),
            confidence: 0.9,
            probabilities: options
                .iter()
                .zip(probabilities)
                .map(|(option, probability)| ((*option).to_owned(), probability))
                .collect(),
            evidence: quote.to_owned(),
            evidence_source: Some(format!("source-{quote}")),
            grounded: true,
            original_choice: None,
            agreement: None,
        }
    }

    fn ungrounded(id: &str, original: &str) -> Answer {
        Answer {
            grounded: false,
            original_choice: Some(original.to_owned()),
            evidence_source: None,
            ..answer(id, "inconnu", [0.0, 0.0, 0.0, 1.0], "inventé")
        }
    }

    fn sample(summary: &str, answers: Vec<Answer>) -> ValidOutput {
        ValidOutput {
            summary: summary.to_owned(),
            answers: ValidAnswers(answers),
        }
    }

    fn voted(agent: &QualificationAgent, samples: &[ValidOutput]) -> ValidOutput {
        vote(agent, samples).unwrap()
    }

    fn near(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn no_sample_gives_no_vote() {
        assert!(vote(&agent(&["q"]), &[]).is_none());
    }

    #[test]
    fn the_majority_choice_wins_and_agreement_is_its_share() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![answer("q", "forte", [0.8, 0.1, 0.1, 0.0], "un")]),
            sample(
                "b",
                vec![answer("q", "moyenne", [0.2, 0.7, 0.1, 0.0], "deux")],
            ),
            sample(
                "c",
                vec![answer("q", "forte", [0.7, 0.2, 0.1, 0.0], "trois")],
            ),
        ];
        let result = voted(&agent, &samples);
        let axis = &result.answers.0[0];
        assert_eq!(axis.choice, "forte");
        assert!(near(axis.agreement.unwrap(), 2.0 / 3.0));
    }

    #[test]
    fn a_tie_goes_to_the_higher_mean_reported_probability() {
        let agent = agent(&["q"]);
        let samples = [
            sample(
                "a",
                vec![answer("q", "forte", [0.55, 0.45, 0.0, 0.0], "un")],
            ),
            sample(
                "b",
                vec![answer("q", "moyenne", [0.1, 0.9, 0.0, 0.0], "deux")],
            ),
        ];
        let result = voted(&agent, &samples);
        assert_eq!(result.answers.0[0].choice, "moyenne");
        assert!(near(result.answers.0[0].agreement.unwrap(), 0.5));
    }

    #[test]
    fn a_full_tie_goes_to_the_first_option() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![answer("q", "faible", [0.0, 0.5, 0.5, 0.0], "un")]),
            sample(
                "b",
                vec![answer("q", "moyenne", [0.0, 0.5, 0.5, 0.0], "deux")],
            ),
        ];
        assert_eq!(voted(&agent, &samples).answers.0[0].choice, "moyenne");
    }

    #[test]
    fn probabilities_blend_the_vote_share_and_the_mean_reported_ones() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![answer("q", "forte", [0.8, 0.2, 0.0, 0.0], "un")]),
            sample(
                "b",
                vec![answer("q", "forte", [0.6, 0.2, 0.2, 0.0], "deux")],
            ),
            sample(
                "c",
                vec![answer("q", "moyenne", [0.1, 0.7, 0.2, 0.0], "trois")],
            ),
        ];
        let result = voted(&agent, &samples);
        let axis = &result.answers.0[0];
        let forte = 0.5 * (2.0 / 3.0) + 0.5 * ((0.8 + 0.6 + 0.1) / 3.0);
        let moyenne = 0.5 * (1.0 / 3.0) + 0.5 * ((0.2 + 0.2 + 0.7) / 3.0);
        let faible = 0.5 * ((0.0 + 0.2 + 0.2) / 3.0);
        assert!(near(probability_of(axis, "forte"), forte));
        assert!(near(probability_of(axis, "moyenne"), moyenne));
        assert!(near(probability_of(axis, "faible"), faible));
        assert!(near(probability_of(axis, "inconnu"), 0.0));
        let sum: f64 = axis.probabilities.iter().map(|(_, p)| p).sum();
        assert!(near(sum, 1.0));
    }

    #[test]
    fn evidence_comes_from_the_first_grounded_sample_that_chose_the_final_option() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![ungrounded("q", "forte")]),
            sample(
                "b",
                vec![answer("q", "forte", [0.9, 0.1, 0.0, 0.0], "deux")],
            ),
            sample(
                "c",
                vec![answer("q", "forte", [0.9, 0.1, 0.0, 0.0], "trois")],
            ),
            sample(
                "d",
                vec![answer("q", "faible", [0.0, 0.0, 1.0, 0.0], "quatre")],
            ),
        ];
        let result = voted(&agent, &samples);
        let axis = &result.answers.0[0];
        assert_eq!(axis.choice, "forte");
        assert_eq!(axis.evidence, "deux");
        assert_eq!(axis.evidence_source.as_deref(), Some("source-deux"));
        assert!(axis.grounded);
        assert!(axis.original_choice.is_none());
    }

    #[test]
    fn the_answer_stays_ungrounded_when_no_supporting_sample_was_grounded() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![ungrounded("q", "forte")]),
            sample("b", vec![ungrounded("q", "moyenne")]),
            sample(
                "c",
                vec![answer("q", "faible", [0.0, 0.0, 1.0, 0.0], "trois")],
            ),
        ];
        let result = voted(&agent, &samples);
        let axis = &result.answers.0[0];
        assert_eq!(axis.choice, "inconnu");
        assert!(!axis.grounded);
        assert_eq!(axis.original_choice.as_deref(), Some("forte"));
        assert!(axis.evidence_source.is_none());
    }

    #[test]
    fn the_axis_is_grounded_when_any_supporting_sample_was() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![ungrounded("q", "forte")]),
            sample("b", vec![answer("q", "inconnu", [0.0, 0.0, 0.0, 1.0], "")]),
        ];
        let result = voted(&agent, &samples);
        assert_eq!(result.answers.0[0].choice, "inconnu");
        assert!(result.answers.0[0].grounded);
        assert!(result.answers.0[0].original_choice.is_none());
    }

    #[test]
    fn the_summary_is_the_first_one_agreeing_most_with_the_final_choices() {
        let agent = agent(&["x", "y"]);
        let samples = [
            sample(
                "premier",
                vec![
                    answer("x", "forte", [1.0, 0.0, 0.0, 0.0], "un"),
                    answer("y", "faible", [0.0, 0.0, 1.0, 0.0], "un"),
                ],
            ),
            sample(
                "deuxieme",
                vec![
                    answer("x", "forte", [1.0, 0.0, 0.0, 0.0], "deux"),
                    answer("y", "moyenne", [0.0, 1.0, 0.0, 0.0], "deux"),
                ],
            ),
            sample(
                "troisieme",
                vec![
                    answer("x", "forte", [1.0, 0.0, 0.0, 0.0], "trois"),
                    answer("y", "moyenne", [0.0, 1.0, 0.0, 0.0], "trois"),
                ],
            ),
        ];
        let result = voted(&agent, &samples);
        assert_eq!(result.answers.0[0].choice, "forte");
        assert_eq!(result.answers.0[1].choice, "moyenne");
        assert_eq!(result.summary, "deuxieme");
        assert!(near(result.answers.0[0].agreement.unwrap(), 1.0));
        assert!(near(result.answers.0[1].agreement.unwrap(), 2.0 / 3.0));
    }

    #[test]
    fn a_single_valid_sample_is_kept_as_it_is_with_full_agreement() {
        let agent = agent(&["q"]);
        let only = sample(
            "seul",
            vec![answer("q", "forte", [0.8, 0.1, 0.1, 0.0], "un")],
        );
        let result = voted(&agent, std::slice::from_ref(&only));
        let axis = &result.answers.0[0];
        assert_eq!(result.summary, "seul");
        assert_eq!(axis.probabilities, only.answers.0[0].probabilities);
        assert!(near(axis.agreement.unwrap(), 1.0));
    }

    #[test]
    fn agreement_survives_the_stored_form() {
        let agent = agent(&["q"]);
        let samples = [
            sample("a", vec![answer("q", "forte", [0.8, 0.1, 0.1, 0.0], "un")]),
            sample(
                "b",
                vec![answer("q", "forte", [0.7, 0.2, 0.1, 0.0], "deux")],
            ),
            sample(
                "c",
                vec![answer("q", "faible", [0.1, 0.1, 0.8, 0.0], "trois")],
            ),
        ];
        let json = voted(&agent, &samples).answers.to_json();
        let stored = crate::domain::qualification::stored_answers(&agent, &json).unwrap();
        assert!(near(stored.0[0].agreement.unwrap(), 2.0 / 3.0));
    }

    #[test]
    fn the_raw_record_keeps_valid_and_invalid_samples() {
        let valid = sample("a", vec![answer("q", "forte", [1.0, 0.0, 0.0, 0.0], "un")]);
        let kept = sample_record(0, Ok(&valid));
        assert_eq!(kept["valid"], true);
        assert_eq!(kept["summary"], "a");
        assert_eq!(kept["answers"]["q"]["choice"], "forte");
        let dropped = sample_record(2, Err("contrat"));
        assert_eq!(dropped["index"], 2);
        assert_eq!(dropped["valid"], false);
        assert_eq!(dropped["error"], "contrat");
    }
}
