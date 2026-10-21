pub fn headcount_label(code: &str) -> Option<&'static str> {
    Some(match code {
        "NN" => "unité non employeuse",
        "00" => "0 salarié",
        "01" => "1 ou 2 salariés",
        "02" => "3 à 5 salariés",
        "03" => "6 à 9 salariés",
        "11" => "10 à 19 salariés",
        "12" => "20 à 49 salariés",
        "21" => "50 à 99 salariés",
        "22" => "100 à 199 salariés",
        "31" => "200 à 249 salariés",
        "32" => "250 à 499 salariés",
        "41" => "500 à 999 salariés",
        "42" => "1 000 à 1 999 salariés",
        "51" => "2 000 à 4 999 salariés",
        "52" => "5 000 à 9 999 salariés",
        "53" => "10 000 salariés et plus",
        _ => return None,
    })
}

pub fn naf_section_label(section: &str) -> Option<&'static str> {
    Some(match section {
        "A" => "Agriculture, sylviculture et pêche",
        "B" => "Industries extractives",
        "C" => "Industrie manufacturière",
        "D" => "Production et distribution d'électricité, de gaz, de vapeur et d'air conditionné",
        "E" => {
            "Production et distribution d'eau, assainissement, gestion des déchets et dépollution"
        }
        "F" => "Construction",
        "G" => "Commerce, réparation d'automobiles et de motocycles",
        "H" => "Transports et entreposage",
        "I" => "Hébergement et restauration",
        "J" => "Information et communication",
        "K" => "Activités financières et d'assurance",
        "L" => "Activités immobilières",
        "M" => "Activités spécialisées, scientifiques et techniques",
        "N" => "Activités de services administratifs et de soutien",
        "O" => "Administration publique",
        "P" => "Enseignement",
        "Q" => "Santé humaine et action sociale",
        "R" => "Arts, spectacles et activités récréatives",
        "S" => "Autres activités de services",
        "T" => "Activités des ménages en tant qu'employeurs",
        "U" => "Activités extra-territoriales",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_have_a_label() {
        assert_eq!(headcount_label("12"), Some("20 à 49 salariés"));
        assert_eq!(headcount_label("99"), None);
        assert_eq!(naf_section_label("J"), Some("Information et communication"));
        assert_eq!(naf_section_label("Z"), None);
    }
}
