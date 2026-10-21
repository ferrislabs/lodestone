use super::{is_hyphen, normalize, phrase_starts};
use crate::domain::text::fold;

const CONTEXT_CHARS: usize = 140;
const MAX_PASSAGES: usize = 3;
const MAX_TOTAL_CHARS: usize = 900;
const MAX_MERGED_CHARS: usize = 450;
const ELLIPSIS: char = '…';

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Span {
    start: usize,
    end: usize,
}

impl Span {
    fn width(self) -> usize {
        self.end - self.start
    }

    fn union(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

struct Folded {
    text: String,
    origins: Vec<usize>,
}

impl Folded {
    fn new(chars: &[char]) -> Self {
        let mut text = String::new();
        let mut origins = Vec::new();
        let mut gap: Option<usize> = None;
        for (index, letter) in chars.iter().enumerate() {
            if letter.is_whitespace() || is_hyphen(*letter) {
                gap.get_or_insert(index);
                continue;
            }
            if let Some(first) = gap.take()
                && !text.is_empty()
            {
                text.push(' ');
                origins.push(first);
            }
            let mut buffer = [0; 4];
            let folded = fold(letter.encode_utf8(&mut buffer));
            origins.resize(origins.len() + folded.len(), index);
            text.push_str(&folded);
        }
        Self { text, origins }
    }

    fn span(&self, start: usize, len: usize) -> Option<Span> {
        let first = self.origins.get(start)?;
        let last = self.origins.get((start + len).checked_sub(1)?)?;
        Some(Span {
            start: *first,
            end: last + 1,
        })
    }
}

fn occurrences(folded: &Folded, keywords: &[String]) -> Vec<Vec<Span>> {
    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();
    for keyword in keywords {
        let needle = normalize(keyword);
        if seen.contains(&needle) {
            continue;
        }
        let spans: Vec<Span> = phrase_starts(&folded.text, &needle)
            .filter_map(|start| folded.span(start, needle.len()))
            .collect();
        seen.push(needle);
        if !spans.is_empty() {
            found.push(spans);
        }
    }
    found
}

fn word_start(chars: &[char], from: usize) -> usize {
    chars[..from]
        .iter()
        .rposition(|letter| letter.is_whitespace())
        .map_or(0, |gap| gap + 1)
}

fn word_end(chars: &[char], from: usize) -> usize {
    chars[from..]
        .iter()
        .position(|letter| letter.is_whitespace())
        .map_or(chars.len(), |gap| from + gap)
}

fn window(chars: &[char], occurrence: Span) -> Span {
    let first_word = word_start(chars, occurrence.start);
    let last_word = word_end(chars, occurrence.end);
    let mut start = occurrence
        .start
        .saturating_sub(CONTEXT_CHARS)
        .min(first_word);
    while start < first_word && start > 0 && !chars[start - 1].is_whitespace() {
        start += 1;
    }
    while start < occurrence.start && chars[start].is_whitespace() {
        start += 1;
    }
    let mut end = (occurrence.end + CONTEXT_CHARS)
        .min(chars.len())
        .max(last_word);
    while end > last_word && end < chars.len() && !chars[end].is_whitespace() {
        end -= 1;
    }
    while end > occurrence.end && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    Span { start, end }
}

fn touches(chars: &[char], first: Span, second: Span) -> bool {
    let (earlier, later) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    later.start <= earlier.end
        || chars[earlier.end..later.start]
            .iter()
            .all(|letter| letter.is_whitespace())
}

fn place(selected: &mut Vec<Span>, window: Span, chars: &[char]) {
    let (touching, apart): (Vec<Span>, Vec<Span>) = selected
        .iter()
        .copied()
        .partition(|span| touches(chars, *span, window));
    let merged = touching
        .iter()
        .fold(window, |merged, span| merged.union(*span));
    let allowed = if touching.is_empty() {
        apart.len() < MAX_PASSAGES
    } else {
        merged.width() <= MAX_MERGED_CHARS
    };
    if allowed {
        *selected = apart;
        selected.push(merged);
        selected.sort();
    }
}

struct Passage {
    leading: bool,
    body: String,
    trailing: bool,
}

impl Passage {
    fn new(chars: &[char], span: Span) -> Self {
        let body: String = chars[span.start..span.end].iter().collect();
        Self {
            leading: chars[..span.start]
                .iter()
                .any(|letter| !letter.is_whitespace()),
            body: body.split_whitespace().collect::<Vec<_>>().join(" "),
            trailing: chars[span.end..]
                .iter()
                .any(|letter| !letter.is_whitespace()),
        }
    }

    fn len(&self) -> usize {
        usize::from(self.leading) + self.body.chars().count() + usize::from(self.trailing)
    }

    fn assemble(&self, body: &str, trailing: bool) -> String {
        let mut text = String::new();
        if self.leading {
            text.push(ELLIPSIS);
        }
        text.push_str(body);
        if trailing {
            text.push(ELLIPSIS);
        }
        text
    }

    fn fitting(&self, budget: usize) -> Option<String> {
        if self.len() <= budget {
            return Some(self.assemble(&self.body, self.trailing));
        }
        let room = budget.checked_sub(usize::from(self.leading) + 1)?;
        let kept: String = self.body.chars().take(room).collect();
        let whole_words =
            kept.ends_with(' ') || self.body.chars().nth(room).is_none_or(|next| next == ' ');
        let cut = if whole_words {
            kept.as_str()
        } else {
            kept.rsplit_once(' ')
                .map_or(kept.as_str(), |(head, _)| head)
        }
        .trim_end();
        (!cut.is_empty()).then(|| self.assemble(cut, true))
    }
}

fn fit(passages: impl Iterator<Item = Passage>) -> Vec<String> {
    let mut remaining = MAX_TOTAL_CHARS;
    let mut kept = Vec::new();
    for passage in passages {
        let Some(text) = passage.fitting(remaining) else {
            break;
        };
        let used = text.chars().count();
        remaining = remaining.saturating_sub(used);
        kept.push(text);
        if used < passage.len() {
            break;
        }
    }
    kept
}

pub fn excerpts(text: &str, keywords: &[String]) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let folded = Folded::new(&chars);
    let found = occurrences(&folded, keywords);
    let mut firsts: Vec<Span> = found
        .iter()
        .filter_map(|spans| spans.first().copied())
        .collect();
    let mut others: Vec<Span> = found
        .iter()
        .flat_map(|spans| spans.iter().skip(1).copied())
        .collect();
    firsts.sort();
    others.sort();
    let mut selected: Vec<Span> = Vec::new();
    for occurrence in firsts.into_iter().chain(others) {
        place(&mut selected, window(&chars, occurrence), &chars);
    }
    fit(selected.iter().map(|span| Passage::new(&chars, *span)))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::domain::{
        profile::definition::{AgentId, SearchAgent},
        triage::{Route, route},
    };

    const FILLER: &str = "Le présent marché porte sur des prestations de service diverses. ";
    const LONG_KEYWORD: &str = "gestion des identités et des accès";

    fn filler(times: usize) -> String {
        FILLER.repeat(times)
    }

    fn keywords(list: &[&str]) -> Vec<String> {
        list.iter().map(|keyword| (*keyword).to_owned()).collect()
    }

    fn total(passages: &[String]) -> usize {
        passages.iter().map(|passage| passage.chars().count()).sum()
    }

    fn assert_whole_words(text: &str, passage: &str) {
        let known: Vec<&str> = text.split_whitespace().collect();
        for word in passage.trim_matches('…').split_whitespace() {
            assert!(known.contains(&word), "{word:?} is not a word of the text");
        }
    }

    #[test]
    fn a_keyword_in_a_long_text_gives_one_passage_around_it() {
        let text = format!(
            "{}Mise en place de l'IDENTITÉ numérique. {}",
            filler(30),
            filler(30)
        );
        assert!(text.chars().count() > 3_900);

        let found = excerpts(&text, &keywords(&["identite"]));

        assert_eq!(found.len(), 1);
        let passage = &found[0];
        assert!(passage.contains("IDENTITÉ"));
        assert!(passage.starts_with('…'));
        assert!(passage.ends_with('…'));
        assert!(passage.chars().count() <= 2 * 140 + "IDENTITÉ".chars().count() + 2);
        assert_whole_words(&text, passage);
    }

    #[test]
    fn two_occurrences_close_together_share_one_passage() {
        let text = format!(
            "{}Le SSO est requis, avec une authentification forte et SSO partout. {}",
            filler(30),
            filler(30)
        );

        let found = excerpts(&text, &keywords(&["sso"]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].matches("SSO").count(), 2);
        assert_whole_words(&text, &found[0]);
    }

    #[test]
    fn five_distinct_keywords_far_apart_give_three_passages_in_text_order() {
        let text: String = ["alpha", "bravo", "charlie", "delta", "echo"]
            .iter()
            .map(|name| format!("{}{name} ", filler(12)))
            .collect();

        let found = excerpts(
            &text,
            &keywords(&["echo", "delta", "charlie", "bravo", "alpha"]),
        );

        assert_eq!(found.len(), 3);
        assert!(found[0].contains("alpha"));
        assert!(found[1].contains("bravo"));
        assert!(found[2].contains("charlie"));
        assert!(total(&found) <= 900);
    }

    #[test]
    fn every_distinct_keyword_is_shown_before_any_is_repeated() {
        let gap = filler(12);
        let text = format!("alpha {gap}alpha {gap}bravo {gap}charlie {gap}");

        let found = excerpts(&text, &keywords(&["alpha", "bravo", "charlie"]));

        assert_eq!(found.len(), 3);
        assert!(found[0].contains("alpha"));
        assert!(found[1].contains("bravo"));
        assert!(found[2].contains("charlie"));
    }

    #[test]
    fn further_occurrences_fill_the_remaining_passages() {
        let gap = filler(12);
        let text = format!("alpha {gap}alpha {gap}bravo {gap}");

        let found = excerpts(&text, &keywords(&["alpha", "bravo"]));

        assert_eq!(found.len(), 3);
        assert!(found[0].contains("alpha"));
        assert!(found[1].contains("alpha"));
        assert!(found[2].contains("bravo"));
    }

    #[test]
    fn the_total_is_capped_by_truncating_the_last_passage() {
        let text: String = (0..3)
            .map(|_| format!("{}{LONG_KEYWORD} ", filler(12)))
            .chain([filler(12)])
            .collect();

        let found = excerpts(&text, &keywords(&[LONG_KEYWORD]));

        assert_eq!(found.len(), 3);
        assert!(total(&found) <= 900);
        assert!(found.iter().all(|passage| passage.contains(LONG_KEYWORD)));
        assert!(found[2].ends_with('…'));
        assert!(found[2].chars().count() < found[1].chars().count());
        assert_whole_words(&text, &found[2]);
    }

    #[test]
    fn keywords_match_without_case_accents_or_hyphens_and_the_passage_keeps_the_original() {
        assert_eq!(
            excerpts("Gestion de l'IDENTITÉ", &keywords(&["identite"])),
            ["Gestion de l'IDENTITÉ"]
        );
        assert_eq!(
            excerpts("Un single sign on d'entreprise", &keywords(&["sign-on"])),
            ["Un single sign on d'entreprise"]
        );
        assert_eq!(
            excerpts("Un single sign-on d'entreprise", &keywords(&["sign on"])),
            ["Un single sign-on d'entreprise"]
        );
    }

    #[test]
    fn whole_words_only_as_in_the_routing() {
        assert!(excerpts("Association des iamistes", &keywords(&["iam"])).is_empty());
        assert!(excerpts("Gestion des identités", &keywords(&["identite"])).is_empty());
        assert_eq!(
            excerpts("Un projet (IAM), urgent", &keywords(&["iam"])),
            ["Un projet (IAM), urgent"]
        );
    }

    #[test]
    fn a_multi_word_keyword_is_kept_whole() {
        let text = format!(
            "{}La gestion des identités et des accès. {}",
            filler(30),
            filler(30)
        );

        let found = excerpts(&text, &keywords(&["Gestion des identités"]));

        assert_eq!(found.len(), 1);
        assert!(found[0].contains("gestion des identités"));
    }

    #[test]
    fn nothing_to_show_gives_nothing() {
        assert!(excerpts("", &keywords(&["sso"])).is_empty());
        assert!(excerpts("   \n ", &keywords(&["sso"])).is_empty());
        assert!(excerpts("Projet SSO", &[]).is_empty());
        assert!(excerpts("Projet SSO", &keywords(&["", "  ", "-"])).is_empty());
        assert!(excerpts("Fourniture de mobilier", &keywords(&["sso"])).is_empty());
    }

    #[test]
    fn a_short_text_is_one_passage_without_ellipses_and_with_collapsed_whitespace() {
        assert_eq!(
            excerpts("Projet   SSO\n\n en  cours", &keywords(&["sso"])),
            ["Projet SSO en cours"]
        );
    }

    #[test]
    fn ellipses_only_mark_what_is_cut() {
        let at_start = format!("SSO obligatoire. {}", filler(30));
        let found = excerpts(&at_start, &keywords(&["sso"]));
        assert!(found[0].starts_with("SSO"));
        assert!(found[0].ends_with('…'));

        let at_end = format!("{}Il faut un SSO", filler(30));
        let found = excerpts(&at_end, &keywords(&["sso"]));
        assert!(found[0].starts_with('…'));
        assert!(found[0].ends_with("SSO"));
    }

    #[test]
    fn offsets_survive_a_folding_that_changes_the_length() {
        let text = format!("{}IAM {}", "œ ".repeat(200), "œ ".repeat(200));

        let found = excerpts(&text, &keywords(&["iam"]));

        assert_eq!(found.len(), 1);
        assert!(found[0].contains(" IAM "));
        assert!(found[0].starts_with('…'));
        assert!(found[0].ends_with('…'));
        assert!(found[0].chars().count() <= 2 * 140 + 3 + 2);
        assert_whole_words(&text, &found[0]);
    }

    fn agent(keywords: &[String]) -> SearchAgent {
        let keywords: Vec<&str> = keywords.iter().map(String::as_str).collect();
        SearchAgent::new(AgentId::new("rules"), "Rules", &["boamp"], &keywords, &[])
    }

    fn assert_same_normalisation(text: &str) -> Result<(), TestCaseError> {
        let chars: Vec<char> = text.chars().collect();
        let folded = Folded::new(&chars);
        prop_assert_eq!(&folded.text, &normalize(text));
        prop_assert_eq!(folded.origins.len(), folded.text.len());
        prop_assert!(folded.origins.windows(2).all(|pair| pair[0] <= pair[1]));
        prop_assert!(folded.origins.iter().all(|origin| *origin < chars.len()));
        Ok(())
    }

    proptest! {
        #[test]
        fn the_offset_aware_folding_is_the_routing_normalisation(
            text in "[a-zA-Zéèœ’İ \\-\t\n\u{2010}\u{2011}\u{a0}]{0,60}",
        ) {
            assert_same_normalisation(&text)?;
        }

        #[test]
        fn the_offset_aware_folding_is_the_routing_normalisation_on_any_text(
            text in "\\PC{0,100}",
        ) {
            assert_same_normalisation(&text)?;
        }

        #[test]
        fn excerpts_never_panic_and_stay_within_bounds(
            text in "\\PC{0,400}",
            keywords in proptest::collection::vec("\\PC{0,8}", 0..5),
        ) {
            let found = excerpts(&text, &keywords);
            prop_assert!(found.len() <= 3);
            prop_assert!(total(&found) <= 900);
        }

        #[test]
        fn passages_are_pieces_of_the_collapsed_text(
            text in "[a-cA-C éœ\\-\n]{0,1200}",
            keywords in proptest::collection::vec("[a-c]{1,3}", 0..4),
        ) {
            let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let found = excerpts(&text, &keywords);
            prop_assert!(found.len() <= 3);
            prop_assert!(total(&found) <= 900);
            for passage in &found {
                prop_assert!(collapsed.contains(passage.trim_matches('…')));
            }
        }

        #[test]
        fn a_keyword_has_a_passage_exactly_when_the_routing_matches_it(
            text in "[a-cA-C éœ\\-\n]{0,300}",
            keywords in proptest::collection::vec("[a-c]{1,3}", 0..4),
        ) {
            let routed = route(&text, &agent(&keywords));
            prop_assert_eq!(
                excerpts(&text, &keywords).is_empty(),
                routed == Route::Reject
            );
        }
    }
}
