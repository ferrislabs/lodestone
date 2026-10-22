use chrono::{DateTime, Utc};

use crate::domain::{
    NormalizedDomain,
    profile::definition::SearchAgent,
    text::fold,
    values::{Provenance, SignalKind, SourceName},
};

pub mod countries;
pub mod excerpt;
pub mod filters;

pub use excerpt::excerpts;
pub use filters::{FilterKind, failing_filter, passes as passes_filters};

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub domain: NormalizedDomain,
    pub source: SourceName,
    pub kind: SignalKind,
    pub title: String,
    pub text: String,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Accept {
        matched: Vec<String>,
    },
    Reject,
    Excluded {
        matched: Vec<String>,
        excluded: Vec<String>,
    },
    Ambiguous {
        matched: Vec<String>,
    },
}

pub(crate) fn is_hyphen(letter: char) -> bool {
    matches!(letter, '-' | '\u{2010}' | '\u{2011}')
}

pub(crate) fn normalize(text: &str) -> String {
    let unhyphenated: String = fold(text)
        .chars()
        .map(|letter| if is_hyphen(letter) { ' ' } else { letter })
        .collect();
    unhyphenated
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn phrase_starts<'a>(
    haystack: &'a str,
    needle: &'a str,
) -> impl Iterator<Item = usize> + 'a {
    let candidates = (!needle.is_empty()).then(|| haystack.match_indices(needle));
    candidates
        .into_iter()
        .flatten()
        .filter_map(move |(start, found)| {
            let before = haystack[..start].chars().next_back();
            let after = haystack[start + found.len()..].chars().next();
            (!before.is_some_and(char::is_alphanumeric)
                && !after.is_some_and(char::is_alphanumeric))
            .then_some(start)
        })
}

fn contains_phrase(haystack: &str, needle: &str) -> bool {
    phrase_starts(haystack, needle).next().is_some()
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

pub fn route(candidate_text: &str, rules: &SearchAgent) -> Route {
    let haystack = normalize(candidate_text);
    let matched = matching(&haystack, &rules.keywords);
    if matched.is_empty() {
        return Route::Reject;
    }
    let excluded = matching(&haystack, &rules.exclusions);
    if !excluded.is_empty() {
        Route::Excluded { matched, excluded }
    } else if matched.len() >= usize::from(rules.accept_min_keywords) {
        Route::Accept { matched }
    } else {
        Route::Ambiguous { matched }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::domain::profile::definition::AgentId;

    fn empty_agent() -> SearchAgent {
        SearchAgent::new(AgentId::new("rules"), "Rules", &["boamp"], &[], &[])
    }

    fn rules(keywords: &[&str], exclusions: &[&str], min: u8) -> SearchAgent {
        SearchAgent {
            accept_min_keywords: min,
            ..SearchAgent::new(
                AgentId::new("rules"),
                "Rules",
                &["boamp"],
                keywords,
                exclusions,
            )
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
                matched: vec!["sso".to_owned()]
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
    fn an_exclusion_rejects_enough_keywords() {
        let route = route("SSO, MFA et badge", &rules(&["sso", "mfa"], &["badge"], 2));
        assert_eq!(
            route,
            Route::Excluded {
                matched: vec!["sso".to_owned(), "mfa".to_owned()],
                excluded: vec!["badge".to_owned()]
            }
        );
    }

    #[test]
    fn an_exclusion_rejects_a_single_keyword_below_threshold() {
        let route = route("Badge et SSO", &rules(&["sso", "mfa"], &["badge"], 2));
        assert_eq!(
            route,
            Route::Excluded {
                matched: vec!["sso".to_owned()],
                excluded: vec!["badge".to_owned()]
            }
        );
    }

    #[test]
    fn an_exclusion_rejects_whatever_the_threshold() {
        assert!(matches!(
            route("Badge et SSO", &rules(&["sso"], &["badge"], 1)),
            Route::Excluded { .. }
        ));
        assert!(matches!(
            route("Badge et SSO", &rules(&["sso"], &["badge"], 0)),
            Route::Excluded { .. }
        ));
    }

    #[test]
    fn an_exclusion_that_is_absent_changes_nothing() {
        assert!(matches!(
            route("SSO et MFA", &rules(&["sso", "mfa"], &["badge"], 2)),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("SSO seul", &rules(&["sso", "mfa"], &["badge"], 2)),
            Route::Ambiguous { .. }
        ));
    }

    #[test]
    fn exclusions_match_without_accents_case_or_hyphens() {
        let rules = rules(&["sso"], &["annuaire téléphonique", "e-mail"], 1);
        assert!(matches!(
            route("SSO et ANNUAIRE TELEPHONIQUE", &rules),
            Route::Excluded { .. }
        ));
        assert!(matches!(
            route("SSO et e mail", &rules),
            Route::Excluded { .. }
        ));
    }

    #[test]
    fn accents_are_ignored_in_either_direction() {
        assert_eq!(
            route("Gestion de l'identité", &rules(&["identite"], &[], 1)),
            Route::Accept {
                matched: vec!["identite".to_owned()]
            }
        );
        assert_eq!(
            route("IDENTITE", &rules(&["identité"], &[], 1)),
            Route::Accept {
                matched: vec!["identité".to_owned()]
            }
        );
    }

    #[test]
    fn accent_folding_keeps_whole_word_matching() {
        assert_eq!(
            route("Gestion des identités", &rules(&["identite"], &[], 1)),
            Route::Reject
        );
    }

    #[test]
    fn hyphens_are_ignored_in_either_direction() {
        let hyphenated = rules(&["sign-on"], &[], 1);
        assert!(matches!(
            route("single sign on", &hyphenated),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("single sign-on", &hyphenated),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("single sign\u{2010}on", &hyphenated),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("single sign\u{2011}on", &hyphenated),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("single sign-on", &rules(&["sign on"], &[], 1)),
            Route::Accept { .. }
        ));
    }

    #[test]
    fn hyphens_still_respect_word_boundaries() {
        let rules = rules(&["sign on"], &[], 1);
        assert_eq!(route("single sign-onboarding", &rules), Route::Reject);
        assert_eq!(route("design-on", &rules), Route::Reject);
    }

    #[test]
    fn punctuation_inside_keywords_is_kept() {
        assert!(matches!(
            route("Développeur C# senior", &rules(&["c#"], &[], 1)),
            Route::Accept { .. }
        ));
        assert!(matches!(
            route("Expert .NET confirmé", &rules(&[".net"], &[], 1)),
            Route::Accept { .. }
        ));
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

    #[test]
    fn matched_keywords_keep_their_original_spelling() {
        assert_eq!(
            route("Mise en place d'un annuaire", &rules(&["Annuaire"], &[], 1)),
            Route::Accept {
                matched: vec!["Annuaire".to_owned()]
            }
        );
    }

    proptest! {
        #[test]
        fn route_never_panics(
            text in "\\PC{0,200}",
            keywords in proptest::collection::vec("\\PC{0,12}", 0..5),
            exclusions in proptest::collection::vec("\\PC{0,12}", 0..5),
            min in 0u8..5,
        ) {
            let rules = SearchAgent {
                keywords,
                exclusions,
                accept_min_keywords: min,
                ..empty_agent()
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
            let base = SearchAgent {
                keywords,
                exclusions: exclusions.clone(),
                accept_min_keywords: min,
                ..empty_agent()
            };
            let mut stricter = base.clone();
            stricter.exclusions.push(extra);
            let before = route(&text, &base);
            let after = route(&text, &stricter);
            let accepted_before = matches!(before, Route::Accept { .. });
            let accepted_after = matches!(after, Route::Accept { .. });
            prop_assert!(accepted_before || !accepted_after);
        }

        #[test]
        fn case_and_hyphens_never_change_the_route(
            text in "[a-cA-C \\-]{0,30}",
            keywords in proptest::collection::vec("[a-c]{1,3}", 0..4),
            exclusions in proptest::collection::vec("[a-c]{1,3}", 0..3),
            min in 1u8..4,
        ) {
            let agent = SearchAgent {
                keywords,
                exclusions,
                accept_min_keywords: min,
                ..empty_agent()
            };
            let reference = route(&text, &agent);
            prop_assert_eq!(&reference, &route(&text.to_uppercase(), &agent));
            prop_assert_eq!(&reference, &route(&text.replace('-', " "), &agent));
        }
    }
}
