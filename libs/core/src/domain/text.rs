fn push_folded(folded: &mut String, letter: char) {
    match letter {
        'à' | 'â' | 'ä' | 'á' => folded.push('a'),
        'é' | 'è' | 'ê' | 'ë' => folded.push('e'),
        'î' | 'ï' | 'í' => folded.push('i'),
        'ô' | 'ö' | 'ó' => folded.push('o'),
        'ù' | 'û' | 'ü' | 'ú' => folded.push('u'),
        'ç' => folded.push('c'),
        'œ' => folded.push_str("oe"),
        '’' => folded.push('\''),
        other => folded.push(other),
    }
}

pub(crate) fn fold_with(text: &str, is_break: impl Fn(char) -> bool) -> String {
    let mut folded = String::with_capacity(text.len());
    let mut gap = false;
    for letter in text.chars().flat_map(char::to_lowercase) {
        if is_break(letter) {
            gap = !folded.is_empty();
            continue;
        }
        if gap {
            folded.push(' ');
            gap = false;
        }
        push_folded(&mut folded, letter);
    }
    folded
}

pub fn fold(text: &str) -> String {
    fold_with(text, char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_ligatures_and_whitespace() {
        assert_eq!(fold("Élève Œuvre  à\nNoël"), "eleve oeuvre a noel");
    }

    #[test]
    fn folds_the_typographic_apostrophe() {
        assert_eq!(fold("L’annuaire"), "l'annuaire");
    }

    #[test]
    fn keeps_hyphens_and_other_punctuation() {
        assert_eq!(fold("Full-Remote, C#"), "full-remote, c#");
    }

    #[test]
    fn empty_and_blank_text_fold_to_nothing() {
        assert_eq!(fold(""), "");
        assert_eq!(fold(" \t\n"), "");
    }

    #[test]
    fn leading_and_trailing_breaks_leave_no_space() {
        assert_eq!(fold("  Élan  "), "elan");
        assert_eq!(
            fold_with("-Single--sign - on-", |letter| letter == '-'
                || letter.is_whitespace()),
            "single sign on"
        );
    }
}
