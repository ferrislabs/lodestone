pub struct CountryEntry {
    pub alpha2: &'static str,
    pub alpha3: &'static str,
    pub names: &'static [&'static str],
}

const fn entry(
    alpha2: &'static str,
    alpha3: &'static str,
    names: &'static [&'static str],
) -> CountryEntry {
    CountryEntry {
        alpha2,
        alpha3,
        names,
    }
}

pub static COUNTRIES: [CountryEntry; 36] = [
    entry("FR", "FRA", &["france"]),
    entry("DE", "DEU", &["allemagne", "germany", "deutschland"]),
    entry("BE", "BEL", &["belgique", "belgium", "belgie"]),
    entry("LU", "LUX", &["luxembourg"]),
    entry("CH", "CHE", &["suisse", "switzerland", "schweiz"]),
    entry("ES", "ESP", &["espagne", "spain", "espana"]),
    entry("IT", "ITA", &["italie", "italy", "italia"]),
    entry("PT", "PRT", &["portugal"]),
    entry("NL", "NLD", &["pays-bas", "netherlands", "nederland"]),
    entry("IE", "IRL", &["irlande", "ireland"]),
    entry(
        "GB",
        "GBR",
        &["royaume-uni", "united kingdom", "angleterre", "england"],
    ),
    entry("AT", "AUT", &["autriche", "austria", "osterreich"]),
    entry("PL", "POL", &["pologne", "poland", "polska"]),
    entry("SE", "SWE", &["suede", "sweden", "sverige"]),
    entry("DK", "DNK", &["danemark", "denmark", "danmark"]),
    entry("FI", "FIN", &["finlande", "finland", "suomi"]),
    entry("NO", "NOR", &["norvege", "norway", "norge"]),
    entry("GR", "GRC", &["grece", "greece"]),
    entry("RO", "ROU", &["roumanie", "romania"]),
    entry("BG", "BGR", &["bulgarie", "bulgaria"]),
    entry("HU", "HUN", &["hongrie", "hungary"]),
    entry("CZ", "CZE", &["tchequie", "czechia", "czech republic"]),
    entry("SK", "SVK", &["slovaquie", "slovakia"]),
    entry("HR", "HRV", &["croatie", "croatia"]),
    entry("SI", "SVN", &["slovenie", "slovenia"]),
    entry("LT", "LTU", &["lituanie", "lithuania"]),
    entry("LV", "LVA", &["lettonie", "latvia"]),
    entry("EE", "EST", &["estonie", "estonia"]),
    entry("MT", "MLT", &["malte", "malta"]),
    entry("CY", "CYP", &["chypre", "cyprus"]),
    entry("IS", "ISL", &["islande", "iceland"]),
    entry("LI", "LIE", &["liechtenstein"]),
    entry("US", "USA", &["etats-unis", "united states", "usa"]),
    entry("CA", "CAN", &["canada"]),
    entry("MA", "MAR", &["maroc", "morocco"]),
    entry("TN", "TUN", &["tunisie", "tunisia"]),
];

pub fn alpha3_of(alpha2: &str) -> Option<&'static str> {
    COUNTRIES
        .iter()
        .find(|country| country.alpha2.eq_ignore_ascii_case(alpha2))
        .map(|country| country.alpha3)
}

pub fn alpha2_of_alpha3(alpha3: &str) -> Option<&'static str> {
    COUNTRIES
        .iter()
        .find(|country| country.alpha3.eq_ignore_ascii_case(alpha3))
        .map(|country| country.alpha2)
}

pub fn alpha2_of_tld(tld: &str) -> Option<&'static str> {
    let tld = if tld.eq_ignore_ascii_case("uk") {
        "gb"
    } else {
        tld
    };
    COUNTRIES
        .iter()
        .find(|country| country.alpha2.eq_ignore_ascii_case(tld))
        .map(|country| country.alpha2)
}
