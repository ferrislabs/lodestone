use thiserror::Error;

use super::{EvidenceQuote, QualificationOutput};
use crate::domain::llm::UntrustedDocument;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EvidenceProblem {
    #[error("assessment cites no evidence")]
    Missing,
    #[error("evidence quote {quote_index} does not appear verbatim in the documents")]
    Unverified { quote_index: usize },
}

pub fn verified_evidence<'a>(
    output: &'a QualificationOutput,
    documents: &[UntrustedDocument<'_>],
) -> Result<Vec<&'a EvidenceQuote>, EvidenceProblem> {
    if output.evidence.is_empty() {
        return Err(EvidenceProblem::Missing);
    }
    if let Some(quote_index) = output
        .evidence
        .iter()
        .position(|quote| !quote.is_verbatim_in(documents))
    {
        return Err(EvidenceProblem::Unverified { quote_index });
    }
    Ok(output.evidence.iter().collect())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::domain::prompt::DraftMessage;

    fn output(evidence: Vec<(u32, &str)>) -> QualificationOutput {
        QualificationOutput {
            summary: String::new(),
            approach_angle: String::new(),
            evidence: evidence
                .into_iter()
                .map(|(document_index, quote)| EvidenceQuote {
                    document_index,
                    quote: quote.to_owned(),
                })
                .collect(),
            draft_message: DraftMessage {
                subject: String::new(),
                body: String::new(),
            },
        }
    }

    fn docs<'a>(contents: &'a [&'a str]) -> Vec<UntrustedDocument<'a>> {
        contents
            .iter()
            .map(|content| UntrustedDocument {
                source: "https://example.com",
                content,
            })
            .collect()
    }

    #[test]
    fn empty_evidence_is_missing() {
        let documents = docs(&["alpha"]);
        assert_eq!(
            verified_evidence(&output(vec![]), &documents),
            Err(EvidenceProblem::Missing)
        );
    }

    #[test]
    fn verbatim_quotes_pass() {
        let documents = docs(&["alpha beta", "gamma"]);
        let out = output(vec![(0, "beta"), (1, "gamma")]);
        assert_eq!(verified_evidence(&out, &documents).unwrap().len(), 2);
    }

    #[test]
    fn one_fabricated_quote_rejects_everything() {
        let documents = docs(&["alpha beta"]);
        let out = output(vec![(0, "alpha"), (0, "delta")]);
        assert_eq!(
            verified_evidence(&out, &documents),
            Err(EvidenceProblem::Unverified { quote_index: 1 })
        );
    }

    #[test]
    fn a_quote_from_the_wrong_document_is_unverified() {
        let documents = docs(&["alpha", "beta"]);
        assert_eq!(
            verified_evidence(&output(vec![(1, "alpha")]), &documents),
            Err(EvidenceProblem::Unverified { quote_index: 0 })
        );
    }

    #[test]
    fn out_of_range_index_is_unverified() {
        let documents = docs(&["alpha"]);
        assert_eq!(
            verified_evidence(&output(vec![(7, "alpha")]), &documents),
            Err(EvidenceProblem::Unverified { quote_index: 0 })
        );
    }

    #[test]
    fn whitespace_is_compared_exactly() {
        let documents = docs(&["alpha  beta"]);
        assert!(verified_evidence(&output(vec![(0, "alpha beta")]), &documents).is_err());
        assert!(verified_evidence(&output(vec![(0, "alpha  beta")]), &documents).is_ok());
    }

    proptest! {
        #[test]
        fn any_substring_passes(
            content in "[a-z]{1,8}( [a-z]{1,8}){0,4}",
            start in 0usize..40,
            len in 1usize..40,
        ) {
            let start = start.min(content.len() - 1);
            let end = (start + len).min(content.len());
            let quote = &content[start..end];
            prop_assume!(!quote.trim().is_empty());
            let contents = [content.as_str()];
            let documents = docs(&contents);
            let out = output(vec![(0, quote)]);
            prop_assert!(verified_evidence(&out, &documents).is_ok());
        }

        #[test]
        fn a_quote_with_an_absent_char_fails(
            content in "[a-y ]{1,30}",
            prefix in "[a-y]{0,5}",
        ) {
            let quote = format!("{prefix}z");
            let contents = [content.as_str()];
            let documents = docs(&contents);
            let out = output(vec![(0, quote.as_str())]);
            prop_assert_eq!(
                verified_evidence(&out, &documents),
                Err(EvidenceProblem::Unverified { quote_index: 0 })
            );
        }
    }
}
