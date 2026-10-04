use chrono::{DateTime, Utc};

use crate::domain::{
    NormalizedDomain,
    profile::definition::KeywordSource,
    values::{Provenance, SignalKind, SourceName},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub domain: NormalizedDomain,
    pub source: SourceName,
    pub kind: SignalKind,
    pub title: String,
    pub text: String,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Accept {
        matched: Vec<String>,
    },
    Reject,
    Ambiguous {
        matched: Vec<String>,
        excluded: Vec<String>,
    },
}

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    haystack.match_indices(needle).any(|(start, found)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + found.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn matching(haystack: &str, terms: &[String]) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    for term in terms {
        let normalized = normalize(term);
        if seen.contains(&normalized) {
            continue;
        }
        if contains_phrase(haystack, &normalized) {
            found.push(term.clone());
        }
        seen.push(normalized);
    }
    found
}

pub fn route(candidate_text: &str, rules: &KeywordSource) -> Route {
    let haystack = normalize(candidate_text);
    let matched = matching(&haystack, &rules.keywords);
    if matched.is_empty() {
        return Route::Reject;
    }
    let excluded = matching(&haystack, &rules.exclusions);
    if excluded.is_empty() && matched.len() >= usize::from(rules.accept_min_keywords) {
        Route::Accept { matched }
    } else {
        Route::Ambiguous { matched, excluded }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn rules(keywords: &[&str], exclusions: &[&str], min: u8) -> KeywordSource {
        KeywordSource {
            keywords: keywords.iter().map(|k| (*k).to_owned()).collect(),
            exclusions: exclusions.iter().map(|k| (*k).to_owned()).collect(),
            accept_min_keywords: min,
        }
    }

    #[test]
    fn no_keyword_rejects() {
        assert_eq!(
            route("Fourniture de mobilier", &rules(&["sso"], &[], 2)),
            Route::Reject
        );
    }

    #[test]
    fn enough_distinct_keywords_accept() {
        let route = route(
            "Mise en place d'un SSO avec MFA",
            &rules(&["SSO", "mfa"], &[], 2),
        );
        assert_eq!(
            route,
            Route::Accept {
                matched: vec!["SSO".to_owned(), "mfa".to_owned()]
            }
        );
    }

    #[test]
    fn one_keyword_below_threshold_is_ambiguous() {
        let route = route("Projet SSO", &rules(&["sso", "mfa"], &[], 2));
        assert_eq!(
            route,
            Route::Ambiguous {
                matched: vec!["sso".to_owned()],
                excluded: Vec::new()
            }
        );
    }

    #[test]
    fn threshold_of_one_accepts_a_single_keyword() {
        assert!(matches!(
            route("Projet SSO", &rules(&["sso"], &[], 1)),
            Route::Accept { .. }
        ));
    }

    #[test]
    fn an_exclusion_makes_enough_keywords_ambiguous() {
        let route = route("SSO, MFA et badge", &rules(&["sso", "mfa"], &["badge"], 2));
        assert_eq!(
            route,
            Route::Ambiguous {
                matched: vec!["sso".to_owned(), "mfa".to_owned()],
                excluded: vec!["badge".to_owned()]
            }
        );
    }

    #[test]
    fn exclusion_without_keyword_still_rejects() {
        assert_eq!(
            route("Badge d'accès", &rules(&["sso"], &["badge"], 1)),
            Route::Reject
        );
    }

    #[test]
    fn keywords_match_whole_words_only() {
        assert_eq!(
            route("Association des iamistes", &rules(&["iam"], &[], 1)),
            Route::Reject
        );
        assert!(matches!(
            route("Un projet (IAM), urgent", &rules(&["iam"], &[], 1)),
            Route::Accept { .. }
        ));
        assert_eq!(route("preSSO", &rules(&["sso"], &[], 1)), Route::Reject);
    }

    #[test]
    fn phrases_match_across_whitespace_and_case() {
        let rules = rules(&["gestion des identités"], &[], 1);
        assert!(matches!(
            route("La GESTION   des\nidentités et accès", &rules),
            Route::Accept { .. }
        ));
        assert_eq!(route("gestion des identitéset", &rules), Route::Reject);
        assert_eq!(route("gestion identités", &rules), Route::Reject);
    }

    #[test]
    fn repeated_or_duplicated_keywords_count_once() {
        assert!(matches!(
            route("sso sso SSO", &rules(&["sso", "SSO"], &[], 2)),
            Route::Ambiguous { .. }
        ));
    }

    proptest! {
        #[test]
        fn route_never_panics(
            text in "\\PC{0,200}",
            keywords in proptest::collection::vec("\\PC{0,12}", 0..5),
            exclusions in proptest::collection::vec("\\PC{0,12}", 0..5),
            min in 0u8..5,
        ) {
            let rules = KeywordSource {
                keywords,
                exclusions,
                accept_min_keywords: min,
            };
            let _ = route(&text, &rules);
        }

        #[test]
        fn adding_an_exclusion_never_accepts(
            text in "[a-c ]{0,30}",
            keywords in proptest::collection::vec("[a-c]{1,3}", 0..4),
            exclusions in proptest::collection::vec("[a-c]{1,3}", 0..3),
            extra in "[a-c]{1,3}",
            min in 1u8..4,
        ) {
            let base = KeywordSource {
                keywords,
                exclusions: exclusions.clone(),
                accept_min_keywords: min,
            };
            let mut stricter = base.clone();
            stricter.exclusions.push(extra);
            let before = route(&text, &base);
            let after = route(&text, &stricter);
            let accepted_before = matches!(before, Route::Accept { .. });
            let accepted_after = matches!(after, Route::Accept { .. });
            prop_assert!(accepted_before || !accepted_after);
        }
    }
}
