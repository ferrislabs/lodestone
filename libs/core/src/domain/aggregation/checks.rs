use std::collections::HashSet;

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::AggregatorOutput;
use crate::domain::{lead::QualityWarning, prompt::normalise::normalise};

pub const MAX_SYNTHESIS_CHARS: usize = 900;
pub const MAX_VERDICT_NARRATIVE_CHARS: usize = 400;
pub const MAX_APPROACH_ANGLE_CHARS: usize = 400;
pub const MAX_DRAFT_SUBJECT_CHARS: usize = 120;
pub const MAX_DRAFT_BODY_CHARS: usize = 2000;

const NEAR_DUPLICATE_SIMILARITY: f64 = 0.8;
const NEAR_DUPLICATE_MIN_WORDS: usize = 4;
const IDENTICAL_MIN_WORDS: usize = 2;
const EXCERPT_CHARS: usize = 90;

const VERDICT_WORDS: [&str; 9] = [
    "confiance",
    "confidence",
    "fiable",
    "reliable",
    "a verifier",
    "to check",
    "ecarte",
    "discarded",
    "verdict",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: QualityWarning,
    pub field: &'static str,
    pub detail: String,
}

struct Field<'a> {
    name: &'static str,
    text: &'a str,
}

fn excerpt(sentence: &str) -> String {
    let mut shown: String = sentence.chars().take(EXCERPT_CHARS).collect();
    if sentence.chars().count() > EXCERPT_CHARS {
        shown.push_str("...");
    }
    shown
}

fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let ends = match c {
            '\n' => true,
            '.' | '!' | '?' | '…' => chars.peek().is_none_or(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if ends {
            let end = index + c.len_utf8();
            found.push(&text[start..end]);
            start = end;
        }
    }
    found.push(&text[start..]);
    found
        .into_iter()
        .map(str::trim)
        .filter(|sentence| sentence.chars().any(char::is_alphanumeric))
        .collect()
}

fn words(sentence: &str) -> Vec<String> {
    normalise(sentence)
        .split_whitespace()
        .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric()).to_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

struct Sentence<'a> {
    field: &'static str,
    text: &'a str,
    words: Vec<String>,
}

impl Sentence<'_> {
    fn repeats(&self, other: &Self) -> bool {
        if self.words.len() >= IDENTICAL_MIN_WORDS && self.words == other.words {
            return true;
        }
        if self.words.len() < NEAR_DUPLICATE_MIN_WORDS
            || other.words.len() < NEAR_DUPLICATE_MIN_WORDS
        {
            return false;
        }
        let left: HashSet<&str> = self.words.iter().map(String::as_str).collect();
        let right: HashSet<&str> = other.words.iter().map(String::as_str).collect();
        let shared = left.intersection(&right).count();
        let total = left.union(&right).count();
        total > 0 && shared as f64 / total as f64 >= NEAR_DUPLICATE_SIMILARITY
    }
}

fn flatten<'a>(fields: &[&Field<'a>]) -> Vec<Sentence<'a>> {
    fields
        .iter()
        .flat_map(|field| {
            sentences(field.text).into_iter().map(|text| Sentence {
                field: field.name,
                text,
                words: words(text),
            })
        })
        .collect()
}

fn repetitions(fields: &[&Field<'_>]) -> Vec<Violation> {
    let all = flatten(fields);
    let mut violations: Vec<Violation> = Vec::new();
    for (index, sentence) in all.iter().enumerate() {
        let Some(earlier) = all[..index].iter().find(|other| sentence.repeats(other)) else {
            continue;
        };
        if violations.iter().any(|known| known.field == sentence.field) {
            continue;
        }
        let place = if earlier.field == sentence.field {
            "earlier in the same field".to_owned()
        } else {
            format!("in `{}`", earlier.field)
        };
        violations.push(Violation {
            kind: QualityWarning::Repetition,
            field: sentence.field,
            detail: format!(
                "repeats a sentence already written {place}: \"{}\"",
                excerpt(sentence.text)
            ),
        });
    }
    violations
}

fn fold_accents(text: &str) -> String {
    normalise(text)
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect()
}

fn has_percentage(folded: &str) -> bool {
    let chars: Vec<char> = folded.chars().collect();
    chars.iter().enumerate().any(|(index, c)| {
        *c == '%'
            && chars[..index]
                .iter()
                .rev()
                .find(|previous| !previous.is_whitespace())
                .is_some_and(char::is_ascii_digit)
    }) || ["pour cent", "pourcent", "percent"]
        .iter()
        .any(|word| folded.contains(word) && folded.chars().any(|c| c.is_ascii_digit()))
}

fn restates_verdict(folded: &str) -> bool {
    folded.chars().any(|c| c.is_ascii_digit())
        && VERDICT_WORDS.iter().any(|word| folded.contains(word))
}

fn restated_figures(field: &Field<'_>) -> Option<Violation> {
    let offending = sentences(field.text).into_iter().find(|sentence| {
        let folded = fold_accents(sentence);
        has_percentage(&folded) || restates_verdict(&folded)
    })?;
    Some(Violation {
        kind: QualityWarning::RestatedFigures,
        field: field.name,
        detail: format!(
            "states a percentage, the confidence or the verdict with a number: \"{}\"",
            excerpt(offending)
        ),
    })
}

fn too_long(field: &Field<'_>, max: usize) -> Option<Violation> {
    let length = field.text.chars().count();
    (length > max).then(|| Violation {
        kind: QualityWarning::TooLong,
        field: field.name,
        detail: format!("has {length} characters, the limit is {max}"),
    })
}

pub fn check(output: &AggregatorOutput) -> Vec<Violation> {
    let summary = Field {
        name: "summary",
        text: &output.summary,
    };
    let narrative = Field {
        name: "verdict_narrative",
        text: &output.verdict_narrative,
    };
    let angle = Field {
        name: "approach_angle",
        text: &output.approach_angle,
    };
    let subject = Field {
        name: "draft_message.subject",
        text: &output.draft_message.subject,
    };
    let body = Field {
        name: "draft_message.body",
        text: &output.draft_message.body,
    };
    let narratives = [&summary, &narrative, &angle];
    let mut violations = repetitions(&narratives);
    violations.extend(repetitions(&[&body]));
    violations.extend(
        narratives
            .iter()
            .filter_map(|field| restated_figures(field)),
    );
    violations.extend(
        [
            (&summary, MAX_SYNTHESIS_CHARS),
            (&narrative, MAX_VERDICT_NARRATIVE_CHARS),
            (&angle, MAX_APPROACH_ANGLE_CHARS),
            (&subject, MAX_DRAFT_SUBJECT_CHARS),
            (&body, MAX_DRAFT_BODY_CHARS),
        ]
        .into_iter()
        .filter_map(|(field, max)| too_long(field, max)),
    );
    violations
}

pub fn warnings(violations: &[Violation]) -> Vec<QualityWarning> {
    let mut kinds: Vec<QualityWarning> = Vec::new();
    for violation in violations {
        if !kinds.contains(&violation.kind) {
            kinds.push(violation.kind);
        }
    }
    kinds
}

pub fn retry_message(violations: &[Violation]) -> String {
    let listed: Vec<String> = violations
        .iter()
        .map(|violation| format!("- `{}` {}", violation.field, violation.detail))
        .collect();
    format!(
        "Your previous answer broke these output rules:\n{}\nAnswer again with only the JSON object. State each idea once, in different words in each field, never write a percentage nor the confidence or the verdict with a number, and stay within the length limits.",
        listed.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::prompt::DraftMessage;

    fn output(summary: &str, narrative: &str, angle: &str, body: &str) -> AggregatorOutput {
        AggregatorOutput {
            summary: summary.to_owned(),
            verdict_narrative: narrative.to_owned(),
            approach_angle: angle.to_owned(),
            evidence: Vec::new(),
            draft_message: DraftMessage {
                subject: "Authentification externalisée".to_owned(),
                body: body.to_owned(),
            },
        }
    }

    fn clean() -> AggregatorOutput {
        output(
            "L'entreprise délègue son authentification à Auth0 sur un hôte de développement.",
            "Un seul signal, issu d'une seule source, ne suffit pas à conclure.",
            "Une gestion d'identité managée pourrait simplifier ses accès.",
            "Bonjour, nous avons remarqué votre usage d'Auth0. Seriez-vous disponible pour échanger ?",
        )
    }

    fn kinds(output: &AggregatorOutput) -> Vec<(QualityWarning, &'static str)> {
        check(output)
            .into_iter()
            .map(|violation| (violation.kind, violation.field))
            .collect()
    }

    #[test]
    fn a_clean_answer_has_no_violation() {
        assert!(check(&clean()).is_empty());
        assert!(warnings(&[]).is_empty());
    }

    #[test]
    fn an_identical_sentence_repeated_in_a_field_is_a_repetition() {
        let mut answer = clean();
        answer.verdict_narrative =
            "Le signal reste isolé. Le signal reste isolé. Il faut vérifier.".to_owned();
        assert_eq!(
            kinds(&answer),
            vec![(QualityWarning::Repetition, "verdict_narrative")]
        );
    }

    #[test]
    fn near_duplicates_are_caught_with_accents_and_case() {
        let mut answer = clean();
        answer.verdict_narrative =
            "Le dispositif est évalué à l'aveugle par les agents. LE DISPOSITIF EST ÉVALUÉ à l’aveugle par les agents, encore."
                .to_owned();
        assert_eq!(
            kinds(&answer),
            vec![(QualityWarning::Repetition, "verdict_narrative")]
        );
        let mut near = clean();
        near.summary = "Le prospect utilise Auth0 comme fournisseur d'identité externalisé sur un hôte de recette. Le prospect utilise Auth0 comme fournisseur d'identité externalisé sur un hôte de test."
            .to_owned();
        assert_eq!(kinds(&near), vec![(QualityWarning::Repetition, "summary")]);
    }

    #[test]
    fn a_sentence_repeated_across_fields_is_a_repetition_on_the_later_field() {
        let mut answer = clean();
        answer.approach_angle = answer.summary.clone();
        let found = check(&answer);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].field, "approach_angle");
        assert!(found[0].detail.contains("`summary`"));
    }

    #[test]
    fn distinct_sentences_sharing_a_few_words_are_fine() {
        let mut answer = clean();
        answer.summary =
            "Le prospect utilise Auth0 sur un hôte de développement. Le prospect n'expose aucun hôte de production."
                .to_owned();
        assert!(check(&answer).is_empty());
    }

    #[test]
    fn decimal_numbers_do_not_split_sentences() {
        assert_eq!(sentences("Version 21.1 stable. Suite").len(), 2);
    }

    #[test]
    fn the_draft_body_is_checked_within_itself_only() {
        let mut answer = clean();
        answer.draft_message.body =
            "Nous proposons un échange. Nous proposons un échange.".to_owned();
        assert_eq!(
            kinds(&answer),
            vec![(QualityWarning::Repetition, "draft_message.body")]
        );
        let mut echoing = clean();
        echoing.draft_message.body = echoing.summary.clone();
        assert!(check(&echoing).is_empty());
    }

    #[test]
    fn a_percentage_is_a_restated_figure() {
        for text in [
            "Le verdict repose sur 100 % de concordance.",
            "Une confiance de 100,0 % est retenue.",
            "Environ 40% des hôtes sont concernés.",
            "Taux de 12 pour cent.",
        ] {
            let mut answer = clean();
            answer.verdict_narrative = text.to_owned();
            assert_eq!(
                kinds(&answer),
                vec![(QualityWarning::RestatedFigures, "verdict_narrative")],
                "{text}"
            );
        }
    }

    #[test]
    fn verdict_wording_with_a_number_is_a_restated_figure_even_with_accents() {
        for text in [
            "Le verdict est fiable avec une confiance de 100,0 %",
            "Le lead est à vérifier, score 51 sur 100.",
            "Lead écarté : 2 signaux seulement.",
            "Le lead est a verifier avec 3 signaux.",
            "Verdict reliable based on 3 signals.",
        ] {
            let mut answer = clean();
            answer.summary = text.to_owned();
            assert_eq!(
                kinds(&answer),
                vec![(QualityWarning::RestatedFigures, "summary")],
                "{text}"
            );
        }
    }

    #[test]
    fn a_number_alone_or_a_verdict_word_alone_is_allowed() {
        let mut answer = clean();
        answer.summary =
            "Trois sous-domaines ont été observés, dont un en développement.".to_owned();
        answer.verdict_narrative = "La preuve est mince, la confiance reste limitée.".to_owned();
        answer.approach_angle = "Le verdict doit rester prudent.".to_owned();
        assert!(check(&answer).is_empty());
    }

    #[test]
    fn every_field_has_its_own_length_limit() {
        let long = |max: usize| "a".repeat(max + 1);
        let mut answer = clean();
        answer.summary = long(MAX_SYNTHESIS_CHARS);
        answer.verdict_narrative = long(MAX_VERDICT_NARRATIVE_CHARS);
        answer.approach_angle = long(MAX_APPROACH_ANGLE_CHARS);
        answer.draft_message.subject = long(MAX_DRAFT_SUBJECT_CHARS);
        answer.draft_message.body = long(MAX_DRAFT_BODY_CHARS);
        let found: Vec<&str> = check(&answer)
            .iter()
            .map(|violation| violation.field)
            .collect();
        assert_eq!(
            found,
            vec![
                "summary",
                "verdict_narrative",
                "approach_angle",
                "draft_message.subject",
                "draft_message.body"
            ]
        );
        let mut at_limit = clean();
        at_limit.summary = "é".repeat(MAX_SYNTHESIS_CHARS);
        assert!(check(&at_limit).is_empty());
    }

    #[test]
    fn warnings_are_deduplicated_in_order() {
        let mut answer = clean();
        answer.summary = "Confiance de 80 %. Confiance de 80 %.".to_owned();
        answer.verdict_narrative = "x".repeat(MAX_VERDICT_NARRATIVE_CHARS + 1);
        let found = check(&answer);
        assert_eq!(
            warnings(&found),
            vec![
                QualityWarning::Repetition,
                QualityWarning::RestatedFigures,
                QualityWarning::TooLong
            ]
        );
    }

    #[test]
    fn the_retry_message_lists_every_violation() {
        let mut answer = clean();
        answer.summary = "Confiance de 80 %. Confiance de 80 %.".to_owned();
        let message = retry_message(&check(&answer));
        assert!(message.contains("- `summary` repeats a sentence"));
        assert!(message.contains("- `summary` states a percentage"));
        assert!(message.contains("Answer again with only the JSON object"));
    }

    #[test]
    fn the_observed_gemma_answer_is_rejected() {
        let mut answer = clean();
        answer.verdict_narrative = "Le lead est jugé fort et pertinent pour l'offre. Le verdict est fiable avec une confiance de 100.0 % et un verdict fiable. Le verdict calculé est fiable avec une confiance de 100.0 % et un verdict fiable. Le verdict est fiable avec une confiance de 100.0 % et un verdict fiable.".to_owned();
        let found = kinds(&answer);
        assert!(found.contains(&(QualityWarning::Repetition, "verdict_narrative")));
        assert!(found.contains(&(QualityWarning::RestatedFigures, "verdict_narrative")));
    }
}
