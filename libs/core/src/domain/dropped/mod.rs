use chrono::{DateTime, Utc};

use crate::domain::{
    NormalizedDomain, UserId,
    lead::{str_enum, uuid_id},
    profile::{ProfileId, definition::AgentId},
    triage::{FilterKind, excerpts},
    values::{Provenance, SignalKind, SourceName},
};

pub mod ports;

uuid_id!(DroppedId);

const OPENING_CHARS: usize = 300;
const ELLIPSIS: char = '…';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropReason {
    NoKeyword,
    Exclusion,
    FilterRemote,
    FilterCountry,
    FilterContract,
}

str_enum!(DropReason {
    NoKeyword => "no_keyword",
    Exclusion => "exclusion",
    FilterRemote => "filter_remote",
    FilterCountry => "filter_country",
    FilterContract => "filter_contract",
});

impl From<FilterKind> for DropReason {
    fn from(filter: FilterKind) -> Self {
        match filter {
            FilterKind::Remote => Self::FilterRemote,
            FilterKind::Country => Self::FilterCountry,
            FilterKind::Contract => Self::FilterContract,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DropReview {
    RightlyDropped,
    ShouldHaveKept,
}

str_enum!(DropReview {
    RightlyDropped => "rightly_dropped",
    ShouldHaveKept => "should_have_kept",
});

#[derive(Debug, Clone, PartialEq)]
pub struct DroppedCandidate {
    pub id: DroppedId,
    pub profile_id: ProfileId,
    pub agent: Option<AgentId>,
    pub source: SourceName,
    pub kind: SignalKind,
    pub domain: NormalizedDomain,
    pub title: String,
    pub excerpts: Vec<String>,
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    pub reason: DropReason,
    pub matched: Vec<String>,
    pub excluded: Vec<String>,
    pub details: serde_json::Value,
    pub dropped_at: DateTime<Utc>,
    pub review: Option<DropReview>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<UserId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DroppedFilter {
    pub reason: Option<DropReason>,
    pub agent: Option<AgentId>,
    pub reviewed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallRow {
    pub profile_id: ProfileId,
    pub agent: Option<AgentId>,
    pub reason: DropReason,
    pub dropped: u64,
    pub reviewed: u64,
    pub should_have_kept: u64,
}

impl RecallRow {
    pub fn share(&self) -> Option<f64> {
        if self.reviewed == 0 {
            return None;
        }
        let wrong = f64::from(u32::try_from(self.should_have_kept).unwrap_or(u32::MAX));
        let reviewed = f64::from(u32::try_from(self.reviewed).unwrap_or(u32::MAX));
        Some(wrong / reviewed)
    }
}

fn opening(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let head: String = collapsed.chars().take(OPENING_CHARS).collect();
    if head.len() == collapsed.len() {
        return Some(collapsed);
    }
    let boundary = collapsed[head.len()..].starts_with(' ') || head.ends_with(' ');
    let kept = match head.rfind(' ') {
        Some(space) if !boundary => &head[..space],
        _ => head.as_str(),
    };
    Some(format!("{}{ELLIPSIS}", kept.trim_end()))
}

pub fn dropped_excerpts(
    candidate_text: &str,
    reason: DropReason,
    matched: &[String],
    excluded: &[String],
) -> Vec<String> {
    match reason {
        DropReason::Exclusion => excerpts(candidate_text, excluded),
        DropReason::NoKeyword => opening(candidate_text).into_iter().collect(),
        DropReason::FilterRemote | DropReason::FilterCountry | DropReason::FilterContract => {
            if matched.is_empty() {
                opening(candidate_text).into_iter().collect()
            } else {
                excerpts(candidate_text, matched)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REASONS: [DropReason; 5] = [
        DropReason::NoKeyword,
        DropReason::Exclusion,
        DropReason::FilterRemote,
        DropReason::FilterCountry,
        DropReason::FilterContract,
    ];

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| (*word).to_owned()).collect()
    }

    fn body(sentence: &str) -> String {
        let filler = "Le présent marché porte sur des prestations diverses. ".repeat(20);
        format!("{filler}{sentence} {filler}")
    }

    #[test]
    fn reasons_round_trip_through_their_stored_names() {
        let names = [
            "no_keyword",
            "exclusion",
            "filter_remote",
            "filter_country",
            "filter_contract",
        ];
        for (reason, name) in REASONS.into_iter().zip(names) {
            assert_eq!(reason.as_str(), name);
            assert_eq!(DropReason::parse(name), Some(reason));
        }
        assert_eq!(DropReason::parse("filter"), None);
        assert_eq!(DropReason::parse(""), None);
    }

    #[test]
    fn reviews_round_trip_through_their_stored_names() {
        for (review, name) in [
            (DropReview::RightlyDropped, "rightly_dropped"),
            (DropReview::ShouldHaveKept, "should_have_kept"),
        ] {
            assert_eq!(review.as_str(), name);
            assert_eq!(DropReview::parse(name), Some(review));
        }
        assert_eq!(DropReview::parse("kept"), None);
    }

    #[test]
    fn a_failing_filter_maps_to_its_reason() {
        assert_eq!(
            DropReason::from(FilterKind::Remote),
            DropReason::FilterRemote
        );
        assert_eq!(
            DropReason::from(FilterKind::Country),
            DropReason::FilterCountry
        );
        assert_eq!(
            DropReason::from(FilterKind::Contract),
            DropReason::FilterContract
        );
    }

    #[test]
    fn an_exclusion_shows_the_passage_around_the_excluded_word() {
        let text = body("On installe un badge d'accès pour les agents.");
        let passages = dropped_excerpts(
            &text,
            DropReason::Exclusion,
            &words(&["sso"]),
            &words(&["badge"]),
        );
        assert_eq!(passages.len(), 1);
        assert!(passages[0].contains("badge d'accès"));
    }

    #[test]
    fn an_exclusion_found_only_in_the_title_has_no_passage() {
        let passages = dropped_excerpts(
            "Marché sans mot interdit",
            DropReason::Exclusion,
            &words(&["sso"]),
            &words(&["badge"]),
        );
        assert!(passages.is_empty());
    }

    #[test]
    fn a_filter_drop_shows_the_passage_around_the_matched_keyword() {
        let text = body("Le projet IAM concerne tous les agents.");
        for reason in [
            DropReason::FilterRemote,
            DropReason::FilterCountry,
            DropReason::FilterContract,
        ] {
            let passages = dropped_excerpts(&text, reason, &words(&["IAM"]), &[]);
            assert_eq!(passages.len(), 1, "{reason:?}");
            assert!(passages[0].contains("Le projet IAM concerne"));
        }
    }

    #[test]
    fn a_filter_drop_without_a_match_opens_with_the_start_of_the_text() {
        let passages = dropped_excerpts(
            "Poste à Lyon,   sur site\n\nCDI",
            DropReason::FilterRemote,
            &[],
            &[],
        );
        assert_eq!(passages, ["Poste à Lyon, sur site CDI"]);
    }

    #[test]
    fn a_text_without_keyword_opens_with_its_first_passage() {
        let passages = dropped_excerpts("Fourniture de mobilier", DropReason::NoKeyword, &[], &[]);
        assert_eq!(passages, ["Fourniture de mobilier"]);
    }

    #[test]
    fn a_long_opening_is_cut_at_a_word_boundary_with_an_ellipsis() {
        let text = "abcdefghij ".repeat(100);
        let passages = dropped_excerpts(&text, DropReason::NoKeyword, &[], &[]);
        let expected = format!("{}…", ["abcdefghij"; 27].join(" "));
        assert_eq!(passages, [expected]);
    }

    #[test]
    fn an_opening_that_ends_on_a_word_keeps_every_word() {
        let text = "mot ".repeat(200);
        let passages = dropped_excerpts(&text, DropReason::NoKeyword, &[], &[]);
        assert_eq!(passages.len(), 1);
        let passage = passages[0].strip_suffix('…').unwrap();
        assert!(passage.split(' ').all(|word| word == "mot"));
        assert!(passage.chars().count() <= OPENING_CHARS);
    }

    #[test]
    fn a_single_very_long_word_is_cut_at_the_limit() {
        let text = "x".repeat(500);
        let passages = dropped_excerpts(&text, DropReason::NoKeyword, &[], &[]);
        assert_eq!(passages, [format!("{}…", "x".repeat(OPENING_CHARS))]);
    }

    #[test]
    fn multibyte_text_is_cut_on_characters() {
        let text = "é".repeat(400);
        let passages = dropped_excerpts(&text, DropReason::NoKeyword, &[], &[]);
        assert_eq!(passages, [format!("{}…", "é".repeat(OPENING_CHARS))]);
    }

    #[test]
    fn an_empty_text_has_no_passage() {
        for reason in REASONS {
            assert!(dropped_excerpts("", reason, &[], &[]).is_empty());
            assert!(dropped_excerpts("  \n\t ", reason, &[], &[]).is_empty());
        }
    }

    #[test]
    fn the_share_of_wrong_drops_needs_reviews() {
        let row = |reviewed, should_have_kept| RecallRow {
            profile_id: ProfileId(uuid::Uuid::nil()),
            agent: None,
            reason: DropReason::NoKeyword,
            dropped: 10,
            reviewed,
            should_have_kept,
        };
        assert_eq!(row(0, 0).share(), None);
        assert_eq!(row(4, 1).share(), Some(0.25));
    }
}
