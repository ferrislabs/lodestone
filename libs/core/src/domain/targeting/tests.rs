use uuid::Uuid;

use super::*;
use crate::domain::{
    company::Siren,
    profile::definition::{CompanyCategory, DepartmentCode, NafPrefix, SectorFilter},
};

fn official() -> OfficialData {
    OfficialData {
        siren: Siren::parse("552081317").unwrap(),
        legal_name: "ACME SAS".to_owned(),
        naf_code: Some("62.01Z".to_owned()),
        naf_label: Some("Information et communication".to_owned()),
        headcount_code: Some("12".to_owned()),
        headcount_range: Some("20 à 49 salariés".to_owned()),
        head_office_city: Some("LYON".to_owned()),
        head_office_postcode: Some("69002".to_owned()),
        head_office_country: Some("France".to_owned()),
        head_office_country_code: Some("FR".to_owned()),
        created_on: None,
        active: Some(true),
        category: Some("PME".to_owned()),
        legal_nature: Some("5710".to_owned()),
        head_office_department: Some("69".to_owned()),
    }
}

fn unknown() -> OfficialData {
    OfficialData {
        naf_code: None,
        naf_label: None,
        headcount_code: None,
        headcount_range: None,
        head_office_city: None,
        head_office_postcode: None,
        head_office_country: None,
        head_office_country_code: None,
        active: None,
        category: None,
        legal_nature: None,
        head_office_department: None,
        ..official()
    }
}

fn headcount(code: &str) -> OfficialData {
    OfficialData {
        headcount_code: Some(code.to_owned()),
        headcount_range: None,
        ..official()
    }
}

fn employees(min: Option<u32>, max: Option<u32>) -> TargetCompanies {
    TargetCompanies {
        min_employees: min,
        max_employees: max,
        ..TargetCompanies::default()
    }
}

fn nafs(prefixes: &[&str]) -> Vec<NafPrefix> {
    prefixes
        .iter()
        .map(|prefix| NafPrefix::new(*prefix))
        .collect()
}

fn departments(codes: &[&str]) -> TargetCompanies {
    TargetCompanies {
        departments: codes
            .iter()
            .map(|code| DepartmentCode::new(*code))
            .collect(),
        ..TargetCompanies::default()
    }
}

fn with_naf(code: Option<&str>) -> OfficialData {
    OfficialData {
        naf_code: code.map(str::to_owned),
        ..official()
    }
}

fn located(
    department: Option<&str>,
    postcode: Option<&str>,
    country_code: Option<&str>,
    country: Option<&str>,
) -> OfficialData {
    OfficialData {
        head_office_department: department.map(str::to_owned),
        head_office_postcode: postcode.map(str::to_owned),
        head_office_country_code: country_code.map(str::to_owned),
        head_office_country: country.map(str::to_owned),
        ..official()
    }
}

#[test]
fn the_reason_texts_are_the_published_ones() {
    assert_eq!(TARGET_REASON_PREFIX, "hors cible");
    assert_eq!(CLOSED_COMPANY_REASON, "entreprise fermée");
}

#[test]
fn a_company_passes_empty_rules() {
    let rules = TargetCompanies::default();
    assert_eq!(target_exclusion(&rules, &official()), None);
    assert_eq!(target_exclusion(&rules, &unknown()), None);
}

#[test]
fn a_closed_company_is_excluded_whatever_the_rules() {
    let closed = OfficialData {
        active: Some(false),
        ..official()
    };
    assert_eq!(
        target_exclusion(&TargetCompanies::default(), &closed).as_deref(),
        Some("entreprise fermée")
    );
    let rules = TargetCompanies {
        min_employees: Some(10_000),
        categories: vec![CompanyCategory::Ge],
        sector: SectorFilter::Public,
        naf_exclude: nafs(&["62"]),
        departments: vec![DepartmentCode::new("75")],
        ..TargetCompanies::default()
    };
    assert_eq!(
        target_exclusion(&rules, &closed).as_deref(),
        Some("entreprise fermée")
    );
}

#[test]
fn an_unknown_administrative_state_is_not_closed() {
    let open = OfficialData {
        active: None,
        ..official()
    };
    assert_eq!(target_exclusion(&TargetCompanies::default(), &open), None);
}

#[test]
fn every_headcount_code_maps_to_its_band() {
    let cases = [
        ("NN", 0, Some(0)),
        ("00", 0, Some(0)),
        ("01", 1, Some(2)),
        ("02", 3, Some(5)),
        ("03", 6, Some(9)),
        ("11", 10, Some(19)),
        ("12", 20, Some(49)),
        ("21", 50, Some(99)),
        ("22", 100, Some(199)),
        ("31", 200, Some(249)),
        ("32", 250, Some(499)),
        ("41", 500, Some(999)),
        ("42", 1000, Some(1999)),
        ("51", 2000, Some(4999)),
        ("52", 5000, Some(9999)),
        ("53", 10_000, None),
    ];
    for (code, low, high) in cases {
        assert_eq!(headcount_band(code), Some((low, high)), "{code}");
    }
    assert_eq!(headcount_band("99"), None);
    assert_eq!(headcount_band(""), None);
}

#[test]
fn a_headcount_band_must_overlap_the_wanted_range() {
    let rules = employees(Some(10), Some(49));
    for code in ["11", "12"] {
        assert_eq!(target_exclusion(&rules, &headcount(code)), None, "{code}");
    }
    for code in ["03", "21", "53", "NN"] {
        assert!(
            target_exclusion(&rules, &headcount(code)).is_some(),
            "{code}"
        );
    }
    let straddling = employees(Some(15), Some(25));
    assert_eq!(target_exclusion(&straddling, &headcount("11")), None);
    assert_eq!(target_exclusion(&straddling, &headcount("12")), None);
    assert!(target_exclusion(&straddling, &headcount("21")).is_some());
    assert!(target_exclusion(&straddling, &headcount("03")).is_some());
}

#[test]
fn bounds_work_alone_and_at_their_edges() {
    let at_least_ten = employees(Some(10), None);
    assert!(target_exclusion(&at_least_ten, &headcount("03")).is_some());
    assert_eq!(target_exclusion(&at_least_ten, &headcount("11")), None);
    assert_eq!(target_exclusion(&at_least_ten, &headcount("53")), None);
    let at_most_five = employees(None, Some(5));
    assert_eq!(target_exclusion(&at_most_five, &headcount("02")), None);
    assert!(target_exclusion(&at_most_five, &headcount("03")).is_some());
    assert_eq!(target_exclusion(&at_most_five, &headcount("01")), None);
    let exactly = employees(Some(3), Some(5));
    assert!(target_exclusion(&exactly, &headcount("01")).is_some());
    assert_eq!(target_exclusion(&exactly, &headcount("02")), None);
    assert!(target_exclusion(&exactly, &headcount("03")).is_some());
}

#[test]
fn non_employer_units_have_no_employee() {
    let none = employees(None, Some(0));
    assert_eq!(target_exclusion(&none, &headcount("NN")), None);
    assert_eq!(target_exclusion(&none, &headcount("00")), None);
    assert!(target_exclusion(&none, &headcount("01")).is_some());
    let some = employees(Some(1), None);
    assert!(target_exclusion(&some, &headcount("NN")).is_some());
    assert!(target_exclusion(&some, &headcount("00")).is_some());
}

#[test]
fn the_open_band_has_no_upper_limit() {
    assert_eq!(
        target_exclusion(&employees(Some(50_000), None), &headcount("53")),
        None
    );
    assert_eq!(
        target_exclusion(&employees(Some(10_000), None), &headcount("53")),
        None
    );
    assert!(target_exclusion(&employees(None, Some(9_999)), &headcount("53")).is_some());
    assert!(target_exclusion(&employees(Some(10_000), None), &headcount("52")).is_some());
}

#[test]
fn an_unknown_or_missing_headcount_passes() {
    let rules = employees(Some(10), Some(49));
    assert_eq!(target_exclusion(&rules, &headcount("99")), None);
    assert_eq!(target_exclusion(&rules, &unknown()), None);
}

#[test]
fn the_headcount_reason_names_the_range_or_the_code() {
    let rules = employees(Some(50), None);
    assert_eq!(
        target_exclusion(&rules, &official()).as_deref(),
        Some("hors cible : effectif 20 à 49 salariés")
    );
    assert_eq!(
        target_exclusion(&rules, &headcount("03")).as_deref(),
        Some("hors cible : effectif 03")
    );
}

#[test]
fn a_listed_category_is_required() {
    let rules = TargetCompanies {
        categories: vec![CompanyCategory::Pme, CompanyCategory::Eti],
        ..TargetCompanies::default()
    };
    assert_eq!(target_exclusion(&rules, &official()), None);
    let large = OfficialData {
        category: Some("GE".to_owned()),
        ..official()
    };
    assert_eq!(
        target_exclusion(&rules, &large).as_deref(),
        Some("hors cible : catégorie GE")
    );
    let lowercase = OfficialData {
        category: Some("eti".to_owned()),
        ..official()
    };
    assert_eq!(target_exclusion(&rules, &lowercase), None);
    assert_eq!(target_exclusion(&rules, &unknown()), None);
}

#[test]
fn the_sector_follows_the_legal_nature() {
    let public = TargetCompanies {
        sector: SectorFilter::Public,
        ..TargetCompanies::default()
    };
    let private = TargetCompanies {
        sector: SectorFilter::Private,
        ..TargetCompanies::default()
    };
    let nature = |code: &str| OfficialData {
        legal_nature: Some(code.to_owned()),
        ..official()
    };
    assert_eq!(
        target_exclusion(&public, &nature("5710")).as_deref(),
        Some("hors cible : secteur privé")
    );
    assert_eq!(target_exclusion(&public, &nature("4110")), None);
    assert_eq!(target_exclusion(&public, &nature("7210")), None);
    assert_eq!(
        target_exclusion(&private, &nature("7210")).as_deref(),
        Some("hors cible : secteur public")
    );
    assert_eq!(
        target_exclusion(&private, &nature("4110")).as_deref(),
        Some("hors cible : secteur public")
    );
    assert_eq!(target_exclusion(&private, &nature("5710")), None);
    assert_eq!(target_exclusion(&private, &nature("1000")), None);
    assert_eq!(target_exclusion(&public, &unknown()), None);
    assert_eq!(target_exclusion(&private, &unknown()), None);
    assert_eq!(target_exclusion(&public, &nature("")), None);
    assert_eq!(target_exclusion(&private, &nature("")), None);
    let any = TargetCompanies::default();
    assert_eq!(target_exclusion(&any, &nature("7210")), None);
}

#[test]
fn an_excluded_naf_prefix_matches_by_start() {
    let excluding = |prefix: &str| TargetCompanies {
        naf_exclude: nafs(&[prefix]),
        ..TargetCompanies::default()
    };
    let staffing = with_naf(Some("78.20Z"));
    for prefix in ["78", "78.2", "78.20", "78.20Z"] {
        assert_eq!(
            target_exclusion(&excluding(prefix), &staffing).as_deref(),
            Some("hors cible : activité 78.20Z"),
            "{prefix}"
        );
    }
    for prefix in ["77", "78.3", "78.1", "79"] {
        assert_eq!(
            target_exclusion(&excluding(prefix), &staffing),
            None,
            "{prefix}"
        );
    }
    assert_eq!(target_exclusion(&excluding("78"), &with_naf(None)), None);
}

#[test]
fn an_included_naf_prefix_is_required_when_the_code_is_known() {
    let rules = TargetCompanies {
        naf_include: nafs(&["62", "70.2"]),
        ..TargetCompanies::default()
    };
    assert_eq!(target_exclusion(&rules, &with_naf(Some("62.01Z"))), None);
    assert_eq!(target_exclusion(&rules, &with_naf(Some("70.22Z"))), None);
    assert_eq!(
        target_exclusion(&rules, &with_naf(Some("78.20Z"))).as_deref(),
        Some("hors cible : activité 78.20Z")
    );
    assert_eq!(
        target_exclusion(&rules, &with_naf(Some("70.10Z"))).as_deref(),
        Some("hors cible : activité 70.10Z")
    );
    assert_eq!(target_exclusion(&rules, &with_naf(None)), None);
}

#[test]
fn an_exclusion_inside_an_inclusion_wins() {
    let rules = TargetCompanies {
        naf_include: nafs(&["78"]),
        naf_exclude: nafs(&["78.20Z"]),
        ..TargetCompanies::default()
    };
    assert_eq!(
        target_exclusion(&rules, &with_naf(Some("78.20Z"))).as_deref(),
        Some("hors cible : activité 78.20Z")
    );
    assert_eq!(target_exclusion(&rules, &with_naf(Some("78.10Z"))), None);
}

#[test]
fn the_department_comes_from_the_register_then_from_the_postcode() {
    let from = |department, postcode| {
        let official = located(department, postcode, Some("FR"), Some("France"));
        department_of(&official).map(str::to_owned)
    };
    assert_eq!(from(Some("75"), Some("69002")).as_deref(), Some("75"));
    assert_eq!(from(Some("2A"), None).as_deref(), Some("2A"));
    assert_eq!(from(None, Some("75008")).as_deref(), Some("75"));
    assert_eq!(from(None, Some("01000")).as_deref(), Some("01"));
    assert_eq!(from(None, Some("20000")).as_deref(), Some("2A"));
    assert_eq!(from(None, Some("20190")).as_deref(), Some("2A"));
    assert_eq!(from(None, Some("20200")).as_deref(), Some("2B"));
    assert_eq!(from(None, Some("20620")).as_deref(), Some("2B"));
    assert_eq!(from(None, Some("97400")).as_deref(), Some("974"));
    assert_eq!(from(None, Some("97100")).as_deref(), Some("971"));
    assert_eq!(from(None, Some("97600")).as_deref(), Some("976"));
    assert_eq!(from(Some(" "), Some("69002")).as_deref(), Some("69"));
    assert_eq!(from(None, None), None);
    assert_eq!(from(None, Some("7500")), None);
    assert_eq!(from(None, Some("75 08")), None);
    assert_eq!(from(None, Some("")), None);
}

#[test]
fn a_foreign_head_office_has_no_department() {
    let germany = located(None, Some("10115"), Some("DE"), Some("ALLEMAGNE"));
    assert_eq!(department_of(&germany), None);
    let abroad = located(Some("75"), Some("75008"), None, Some("ALLEMAGNE"));
    assert_eq!(department_of(&abroad), None);
    let france = located(None, Some("75008"), None, Some("France"));
    assert_eq!(department_of(&france), Some("75"));
    let unspecified = located(None, Some("75008"), None, None);
    assert_eq!(department_of(&unspecified), Some("75"));
}

#[test]
fn a_listed_department_is_required() {
    let rules = departments(&["75", "2A", "974"]);
    let in_paris = located(None, Some("75008"), Some("FR"), Some("France"));
    assert_eq!(target_exclusion(&rules, &in_paris), None);
    assert_eq!(
        target_exclusion(&rules, &official()).as_deref(),
        Some("hors cible : département 69")
    );
    let corsica = located(None, Some("20000"), Some("FR"), Some("France"));
    assert_eq!(target_exclusion(&rules, &corsica), None);
    let upper_corsica = located(None, Some("20200"), Some("FR"), Some("France"));
    assert_eq!(
        target_exclusion(&rules, &upper_corsica).as_deref(),
        Some("hors cible : département 2B")
    );
    let reunion = located(None, Some("97400"), Some("FR"), Some("France"));
    assert_eq!(target_exclusion(&rules, &reunion), None);
    let lowercase = located(Some("2a"), None, Some("FR"), Some("France"));
    assert_eq!(target_exclusion(&rules, &lowercase), None);
}

#[test]
fn an_unknown_or_foreign_department_passes() {
    let rules = departments(&["75"]);
    assert_eq!(target_exclusion(&rules, &unknown()), None);
    let germany = located(None, Some("10115"), Some("DE"), Some("ALLEMAGNE"));
    assert_eq!(target_exclusion(&rules, &germany), None);
    let nowhere = located(None, None, None, None);
    assert_eq!(target_exclusion(&rules, &nowhere), None);
}

#[test]
fn the_rules_are_checked_in_the_published_order() {
    let everything = TargetCompanies {
        min_employees: Some(500),
        categories: vec![CompanyCategory::Ge],
        sector: SectorFilter::Public,
        naf_include: nafs(&["35"]),
        naf_exclude: nafs(&["62"]),
        departments: vec![DepartmentCode::new("75")],
        ..TargetCompanies::default()
    };
    let mut company = official();
    assert_eq!(
        target_exclusion(&everything, &company).as_deref(),
        Some("hors cible : effectif 20 à 49 salariés")
    );
    company.headcount_code = Some("42".to_owned());
    company.headcount_range = Some("1 000 à 1 999 salariés".to_owned());
    assert_eq!(
        target_exclusion(&everything, &company).as_deref(),
        Some("hors cible : catégorie PME")
    );
    company.category = Some("GE".to_owned());
    assert_eq!(
        target_exclusion(&everything, &company).as_deref(),
        Some("hors cible : secteur privé")
    );
    company.legal_nature = Some("4110".to_owned());
    assert_eq!(
        target_exclusion(&everything, &company).as_deref(),
        Some("hors cible : activité 62.01Z")
    );
    company.naf_code = Some("35.11Z".to_owned());
    assert_eq!(
        target_exclusion(&everything, &company).as_deref(),
        Some("hors cible : département 69")
    );
    company.head_office_department = Some("75".to_owned());
    assert_eq!(target_exclusion(&everything, &company), None);
}

#[test]
fn target_reasons_are_recognised_and_other_reasons_are_not() {
    assert!(is_target_reason("hors cible : activité 78.20Z"));
    assert!(is_target_reason("hors cible"));
    assert!(is_target_reason("entreprise fermée"));
    assert!(!is_target_reason("hors pays visé"));
    assert!(!is_target_reason("domaine d'un fournisseur SaaS"));
    assert!(!is_target_reason(""));
    assert!(!is_target_reason("entreprise fermée depuis 2020"));
}

#[test]
fn a_sweep_ruling_excludes_restores_replaces_or_keeps() {
    let staffing = TargetCompanies {
        naf_exclude: nafs(&["62"]),
        ..TargetCompanies::default()
    };
    let none = TargetCompanies::default();
    let software = official();
    assert_eq!(
        ruling(&staffing, None, &software),
        Ruling::Exclude("hors cible : activité 62.01Z".to_owned())
    );
    assert_eq!(ruling(&none, None, &software), Ruling::Keep);
    assert_eq!(
        ruling(&none, Some("hors cible : activité 62.01Z"), &software),
        Ruling::Restore("hors cible : activité 62.01Z".to_owned())
    );
    assert_eq!(
        ruling(&staffing, Some("hors cible : activité 62.01Z"), &software),
        Ruling::Keep
    );
    let sized = TargetCompanies {
        min_employees: Some(50),
        ..staffing.clone()
    };
    assert_eq!(
        ruling(&sized, Some("hors cible : activité 62.01Z"), &software),
        Ruling::Replace {
            stored: "hors cible : activité 62.01Z".to_owned(),
            reason: "hors cible : effectif 20 à 49 salariés".to_owned(),
        }
    );
    let closed = OfficialData {
        active: Some(false),
        ..official()
    };
    assert_eq!(
        ruling(&none, Some("entreprise fermée"), &closed),
        Ruling::Keep
    );
    assert_eq!(
        ruling(&none, Some("entreprise fermée"), &software),
        Ruling::Restore("entreprise fermée".to_owned())
    );
}

#[test]
fn a_sweep_ruling_never_touches_another_reason() {
    let staffing = TargetCompanies {
        naf_exclude: nafs(&["62"]),
        ..TargetCompanies::default()
    };
    let software = official();
    for stored in ["hors pays visé", "domaine d'un fournisseur SaaS"] {
        assert_eq!(ruling(&staffing, Some(stored), &software), Ruling::Keep);
        assert_eq!(
            ruling(&TargetCompanies::default(), Some(stored), &software),
            Ruling::Keep
        );
    }
}

#[test]
fn a_targeted_lead_keeps_its_lead_and_its_stored_reason() {
    let targeted = TargetedLead {
        lead_id: LeadId(Uuid::nil()),
        exclusion: Some("hors pays visé".to_owned()),
        official: official(),
    };
    assert_eq!(targeted.lead_id, LeadId(Uuid::nil()));
    assert_eq!(
        ruling(
            &TargetCompanies::default(),
            targeted.exclusion.as_deref(),
            &targeted.official
        ),
        Ruling::Keep
    );
}
