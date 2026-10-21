use super::EvidenceQuote;
use crate::domain::llm::UntrustedDocument;

#[derive(Debug, PartialEq, Eq)]
pub struct EvidenceCheck<'a> {
    pub verified: Vec<&'a EvidenceQuote>,
    pub rejected: usize,
}

impl EvidenceCheck<'_> {
    pub fn is_sound(&self) -> bool {
        self.rejected == 0 && !self.verified.is_empty()
    }
}

pub fn check_evidence<'a>(
    quotes: &'a [EvidenceQuote],
    documents: &[UntrustedDocument<'_>],
) -> EvidenceCheck<'a> {
    let (verified, rejected): (Vec<&EvidenceQuote>, Vec<&EvidenceQuote>) = quotes
        .iter()
        .partition(|quote| quote.is_verbatim_in(documents));
    EvidenceCheck {
        verified,
        rejected: rejected.len(),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn quotes(evidence: Vec<(u32, &str)>) -> Vec<EvidenceQuote> {
        evidence
            .into_iter()
            .map(|(document_index, quote)| EvidenceQuote {
                document_index,
                quote: quote.to_owned(),
            })
            .collect()
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
    fn no_quote_is_not_sound() {
        let documents = docs(&["alpha"]);
        let given = quotes(vec![]);
        let check = check_evidence(&given, &documents);
        assert!(check.verified.is_empty());
        assert!(!check.is_sound());
    }

    #[test]
    fn verbatim_quotes_are_sound() {
        let documents = docs(&["alpha beta", "gamma"]);
        let given = quotes(vec![(0, "beta"), (1, "gamma")]);
        let check = check_evidence(&given, &documents);
        assert_eq!(check.verified.len(), 2);
        assert!(check.is_sound());
    }

    #[test]
    fn a_fabricated_quote_is_dropped_and_makes_the_check_unsound() {
        let documents = docs(&["alpha beta"]);
        let given = quotes(vec![(0, "alpha"), (0, "delta")]);
        let check = check_evidence(&given, &documents);
        assert_eq!(check.verified.len(), 1);
        assert_eq!(check.verified[0].quote, "alpha");
        assert_eq!(check.rejected, 1);
        assert!(!check.is_sound());
    }

    #[test]
    fn a_quote_from_the_wrong_or_missing_document_is_rejected() {
        let documents = docs(&["alpha", "beta"]);
        let given = quotes(vec![(1, "alpha"), (7, "alpha")]);
        let check = check_evidence(&given, &documents);
        assert!(check.verified.is_empty());
        assert_eq!(check.rejected, 2);
    }

    #[test]
    fn typography_is_normalised_but_words_stay_strict() {
        let documents = docs(&["alpha  beta mise en oeuvre"]);
        let sound = |quote: &str| {
            let given = quotes(vec![(0, quote)]);
            check_evidence(&given, &documents).is_sound()
        };
        assert!(sound("alpha beta"));
        assert!(sound("en œuvre"));
        assert!(!sound("alpha gamma"));
    }

    const LUZERN_RAW: &str = "signal: saas_provider\nvalue: auth0\nsource: dns_cname\nobserved: 2026-10-05T23:38:01Z\n{\n  \"host\": \"login.dev.luzernrisk.com\"\n}";
    const LUZERN_SENTENCE: &str = "login.dev.luzernrisk.com délègue son authentification à Auth0 (CNAME DNS)\nsource: dns_cname\nobserved: 2026-10-05T23:38:01Z";

    #[test]
    fn the_luzernrisk_raw_signal_lines_quote_verifies_across_lines() {
        let documents = docs(&[LUZERN_RAW]);
        let given = quotes(vec![(
            0,
            "signal: saas_provider value: auth0 source: dns_cname observed: 2026-10-05T23:38:01Z",
        )]);
        assert!(check_evidence(&given, &documents).is_sound());
        let with_newlines = quotes(vec![(
            0,
            "signal: saas_provider\nvalue: auth0\nsource: dns_cname\nobserved: 2026-10-05T23:38:01Z",
        )]);
        assert!(check_evidence(&with_newlines, &documents).is_sound());
    }

    #[test]
    fn the_luzernrisk_sentence_quote_verifies_alone_or_with_its_metadata_lines() {
        let documents = docs(&[LUZERN_SENTENCE]);
        for quote in [
            "délègue son authentification à Auth0",
            "login.dev.luzernrisk.com délègue son authentification à Auth0 (CNAME DNS) source: dns_cname",
            "source: dns_cname\nobserved: 2026-10-05T23:38:01Z",
            "SOURCE : dns_cname Observed: 2026-10-05t23:38:01z",
        ] {
            let given = quotes(vec![(0, quote)]);
            assert!(check_evidence(&given, &documents).is_sound(), "{quote}");
        }
    }

    #[test]
    fn the_former_raw_line_layout_no_longer_matches_a_sentence_document() {
        let documents = docs(&[LUZERN_SENTENCE]);
        let given = quotes(vec![(
            0,
            "signal: saas_provider value: auth0 source: dns_cname observed: 2026-10-05T23:38:01Z",
        )]);
        let check = check_evidence(&given, &documents);
        assert_eq!(check.rejected, 1);
        assert!(check.verified.is_empty());
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
            let given = quotes(vec![(0, quote)]);
            prop_assert!(check_evidence(&given, &documents).is_sound());
        }

        #[test]
        fn a_quote_with_an_absent_char_fails(
            content in "[a-y ]{1,30}",
            prefix in "[a-y]{0,5}",
        ) {
            let quote = format!("{prefix}z");
            let contents = [content.as_str()];
            let documents = docs(&contents);
            let given = quotes(vec![(0, quote.as_str())]);
            let check = check_evidence(&given, &documents);
            prop_assert_eq!(check.rejected, 1);
        }
    }
}
