use crate::domain::NormalizedDomain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Country {
    pub code: &'static str,
    pub name: &'static str,
}

const COUNTRIES: &[(&str, Country)] = &[
    (
        "fr",
        Country {
            code: "FR",
            name: "France",
        },
    ),
    (
        "de",
        Country {
            code: "DE",
            name: "Germany",
        },
    ),
    (
        "uk",
        Country {
            code: "GB",
            name: "United Kingdom",
        },
    ),
    (
        "gb",
        Country {
            code: "GB",
            name: "United Kingdom",
        },
    ),
    (
        "be",
        Country {
            code: "BE",
            name: "Belgium",
        },
    ),
    (
        "ch",
        Country {
            code: "CH",
            name: "Switzerland",
        },
    ),
    (
        "nl",
        Country {
            code: "NL",
            name: "Netherlands",
        },
    ),
    (
        "es",
        Country {
            code: "ES",
            name: "Spain",
        },
    ),
    (
        "it",
        Country {
            code: "IT",
            name: "Italy",
        },
    ),
    (
        "pt",
        Country {
            code: "PT",
            name: "Portugal",
        },
    ),
    (
        "lu",
        Country {
            code: "LU",
            name: "Luxembourg",
        },
    ),
    (
        "ie",
        Country {
            code: "IE",
            name: "Ireland",
        },
    ),
    (
        "at",
        Country {
            code: "AT",
            name: "Austria",
        },
    ),
    (
        "se",
        Country {
            code: "SE",
            name: "Sweden",
        },
    ),
    (
        "no",
        Country {
            code: "NO",
            name: "Norway",
        },
    ),
    (
        "dk",
        Country {
            code: "DK",
            name: "Denmark",
        },
    ),
    (
        "fi",
        Country {
            code: "FI",
            name: "Finland",
        },
    ),
    (
        "pl",
        Country {
            code: "PL",
            name: "Poland",
        },
    ),
    (
        "cz",
        Country {
            code: "CZ",
            name: "Czechia",
        },
    ),
    (
        "ro",
        Country {
            code: "RO",
            name: "Romania",
        },
    ),
    (
        "gr",
        Country {
            code: "GR",
            name: "Greece",
        },
    ),
    (
        "hu",
        Country {
            code: "HU",
            name: "Hungary",
        },
    ),
    (
        "ca",
        Country {
            code: "CA",
            name: "Canada",
        },
    ),
    (
        "us",
        Country {
            code: "US",
            name: "United States",
        },
    ),
    (
        "mx",
        Country {
            code: "MX",
            name: "Mexico",
        },
    ),
    (
        "br",
        Country {
            code: "BR",
            name: "Brazil",
        },
    ),
    (
        "au",
        Country {
            code: "AU",
            name: "Australia",
        },
    ),
    (
        "nz",
        Country {
            code: "NZ",
            name: "New Zealand",
        },
    ),
    (
        "jp",
        Country {
            code: "JP",
            name: "Japan",
        },
    ),
    (
        "kr",
        Country {
            code: "KR",
            name: "South Korea",
        },
    ),
    (
        "cn",
        Country {
            code: "CN",
            name: "China",
        },
    ),
    (
        "in",
        Country {
            code: "IN",
            name: "India",
        },
    ),
    (
        "sg",
        Country {
            code: "SG",
            name: "Singapore",
        },
    ),
    (
        "ae",
        Country {
            code: "AE",
            name: "United Arab Emirates",
        },
    ),
    (
        "tr",
        Country {
            code: "TR",
            name: "Turkey",
        },
    ),
    (
        "ru",
        Country {
            code: "RU",
            name: "Russia",
        },
    ),
    (
        "za",
        Country {
            code: "ZA",
            name: "South Africa",
        },
    ),
    (
        "ma",
        Country {
            code: "MA",
            name: "Morocco",
        },
    ),
    (
        "tn",
        Country {
            code: "TN",
            name: "Tunisia",
        },
    ),
    (
        "dz",
        Country {
            code: "DZ",
            name: "Algeria",
        },
    ),
    (
        "sn",
        Country {
            code: "SN",
            name: "Senegal",
        },
    ),
    (
        "ci",
        Country {
            code: "CI",
            name: "Ivory Coast",
        },
    ),
    (
        "cm",
        Country {
            code: "CM",
            name: "Cameroon",
        },
    ),
    (
        "ng",
        Country {
            code: "NG",
            name: "Nigeria",
        },
    ),
    (
        "il",
        Country {
            code: "IL",
            name: "Israel",
        },
    ),
    (
        "ar",
        Country {
            code: "AR",
            name: "Argentina",
        },
    ),
];

impl Country {
    pub fn from_code(code: &str) -> Option<Self> {
        COUNTRIES
            .iter()
            .find(|(_, country)| country.code.eq_ignore_ascii_case(code))
            .map(|(_, country)| *country)
    }
}

pub fn country_of(domain: &NormalizedDomain) -> Option<Country> {
    let tld = domain.as_str().rsplit('.').next()?;
    COUNTRIES
        .iter()
        .find(|(candidate, _)| *candidate == tld)
        .map(|(_, country)| *country)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn country(domain: &str) -> Option<Country> {
        country_of(&NormalizedDomain::from_host(domain).unwrap())
    }

    #[test]
    fn a_country_code_tld_names_the_country() {
        assert_eq!(country("acme.fr").unwrap().code, "FR");
        assert_eq!(country("acme.de").unwrap().name, "Germany");
        assert_eq!(country("acme.ch").unwrap().code, "CH");
    }

    #[test]
    fn a_country_is_found_from_its_iso_code() {
        assert_eq!(Country::from_code("fr").unwrap().name, "France");
        assert_eq!(Country::from_code("GB").unwrap().name, "United Kingdom");
        assert_eq!(Country::from_code("ZZ"), None);
    }

    #[test]
    fn the_uk_has_two_tlds_and_one_country() {
        assert_eq!(country("acme.co.uk").unwrap().code, "GB");
        assert_eq!(country("acme.uk").unwrap().code, "GB");
        assert_eq!(country("acme.gb").unwrap().code, "GB");
    }

    #[test]
    fn a_generic_tld_names_nothing() {
        for domain in [
            "acme.com", "acme.org", "acme.net", "acme.io", "acme.ai", "acme.eu",
        ] {
            assert_eq!(country(domain), None, "{domain}");
        }
    }

    #[test]
    fn the_table_has_no_duplicate_tld() {
        for (index, (tld, _)) in COUNTRIES.iter().enumerate() {
            assert!(
                COUNTRIES[index + 1..].iter().all(|(other, _)| other != tld),
                "{tld}"
            );
        }
    }
}
