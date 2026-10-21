pub fn fold(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    for letter in text.chars().flat_map(char::to_lowercase) {
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
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
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
}
