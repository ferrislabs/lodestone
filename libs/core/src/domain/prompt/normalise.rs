use unicode_normalization::UnicodeNormalization;

fn fold_char(c: char, out: &mut String) {
    match c {
        'œ' => out.push_str("oe"),
        'Œ' => out.push_str("OE"),
        'æ' => out.push_str("ae"),
        'Æ' => out.push_str("AE"),
        '’' | '‘' | '‚' | '‛' | '´' | '`' => out.push('\''),
        '“' | '”' | '„' | '«' | '»' | '‹' | '›' => out.push('"'),
        '‐' | '‑' | '‒' | '–' | '—' | '―' | '−' => out.push('-'),
        '…' => out.push_str("..."),
        c if c.is_whitespace() => out.push(' '),
        c => out.push(c),
    }
}

fn collapse_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == ' ' && out.ends_with(' ') {
            continue;
        }
        out.push(c);
    }
    out
}

fn hugs_punctuation(c: char) -> bool {
    matches!(c, '"' | ';' | ':' | '!' | '?')
}

fn drop_punctuation_spacing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ' ' {
            let after_quote = out.ends_with('"');
            let before_mark = chars.peek().is_some_and(|next| hugs_punctuation(*next));
            if after_quote || before_mark {
                continue;
            }
        }
        out.push(c);
    }
    out
}

pub fn normalise(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for c in text.nfkc() {
        fold_char(c, &mut folded);
    }
    let lowered = folded.to_lowercase();
    let collapsed = collapse_spaces(&lowered);
    drop_punctuation_spacing(collapsed.trim()).trim().to_owned()
}

pub fn has_content(normalised: &str) -> bool {
    normalised.chars().any(char::is_alphanumeric)
}

pub fn contains_normalised(document: &str, quote: &str) -> bool {
    let quote = normalise(quote);
    has_content(&quote) && normalise(document).contains(&quote)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn matches(document: &str, quote: &str) -> bool {
        contains_normalised(document, quote)
    }

    #[test]
    fn ligatures_match_both_ways() {
        assert!(matches("mise en oeuvre", "mise en œuvre"));
        assert!(matches("mise en œuvre", "mise en oeuvre"));
        assert!(matches("Œuvre", "oeuvre"));
        assert!(matches("cæsar", "caesar"));
    }

    #[test]
    fn apostrophes_match() {
        assert!(matches("l'outil", "l’outil"));
        assert!(matches("l’outil", "l'outil"));
    }

    #[test]
    fn guillemets_match_straight_quotes() {
        assert!(matches("il dit \"texte\" ici", "« texte »"));
        assert!(matches("il dit « texte » ici", "\"texte\""));
    }

    #[test]
    fn dashes_match() {
        assert!(matches("a-b", "a–b"));
        assert!(matches("a—b", "a-b"));
        assert!(matches("a−b", "a‑b"));
    }

    #[test]
    fn whitespace_variants_match() {
        assert!(matches("alpha beta", "alpha\u{a0}beta"));
        assert!(matches("alpha  beta", "alpha beta"));
        assert!(matches("alpha\n\tbeta", "alpha beta"));
        assert!(matches("alpha\u{202f}beta", "alpha\u{2009}beta"));
    }

    #[test]
    fn french_punctuation_spacing_matches() {
        assert!(matches("Attention : danger !", "attention: danger!"));
    }

    #[test]
    fn case_is_ignored() {
        assert!(matches("KEYCLOAK", "keycloak"));
        assert!(matches("élan", "ÉLAN"));
    }

    #[test]
    fn ellipsis_matches_three_dots() {
        assert!(matches("suite...", "suite…"));
        assert!(matches("suite…", "suite..."));
    }

    #[test]
    fn observed_boamp_quote_passes() {
        assert!(matches(
            "Achat et mise en oeuvre d'une solution IAM en mode SAAS…",
            "Achat et mise en œuvre d'une solution IAM en mode SAAS…"
        ));
    }

    #[test]
    fn a_changed_word_is_rejected() {
        assert!(!matches(
            "Achat et mise en oeuvre d'une solution IAM",
            "Achat et mise en oeuvre d'une solution IGA"
        ));
    }

    #[test]
    fn quotes_without_content_are_rejected() {
        assert!(!matches("a ... b", "..."));
        assert!(!matches("a ... b", "  \u{a0} "));
        assert!(!matches("a ... b", "« » ; !"));
        assert!(!matches("a ... b", ""));
    }

    proptest! {
        #[test]
        fn any_substring_still_passes(
            content in "[a-zA-Zœé’'«»\"–;:! \u{a0}\n]{1,40}",
            start in 0usize..40,
            len in 1usize..40,
        ) {
            let chars: Vec<char> = content.chars().collect();
            let start = start.min(chars.len() - 1);
            let end = (start + len).min(chars.len());
            let quote: String = chars[start..end].iter().collect();
            prop_assume!(has_content(&normalise(&quote)));
            prop_assert!(contains_normalised(&content, &quote));
        }

        #[test]
        fn inserting_an_absent_letter_fails(
            content in "[a-y ]{1,30}",
            prefix in "[a-y]{0,5}",
        ) {
            let quote = format!("{prefix}z");
            prop_assert!(!contains_normalised(&content, &quote));
        }
    }
}
