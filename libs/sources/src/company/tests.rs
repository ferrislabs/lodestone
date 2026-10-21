use super::*;

fn domain() -> NormalizedDomain {
    NormalizedDomain::from_host("acme.fr").unwrap()
}

fn home() -> Url {
    Url::parse("https://www.acme.fr/").unwrap()
}

fn urls(links: &[PageLink]) -> Vec<String> {
    links.iter().map(|link| link.url.to_string()).collect()
}

#[test]
fn links_are_resolved_classified_and_ranked_legal_first() {
    let html = r##"
        <a href="/a-propos">Qui sommes nous</a>
        <a href='/mentions-legales.html'>Mentions</a>
        <a href="https://www.acme.fr/cgu?lang=fr#top">CGU</a>
        <a class="x" href="/produits">Produits</a>
        <a href="mailto:contact@acme.fr">Mail</a>
    "##;
    let links = page_links(html, &home(), &domain());
    assert_eq!(
        urls(&links),
        [
            "https://www.acme.fr/mentions-legales.html",
            "https://www.acme.fr/cgu",
            "https://www.acme.fr/a-propos",
        ]
    );
    assert_eq!(
        links.iter().map(|link| link.kind).collect::<Vec<_>>(),
        [PageKind::Legal, PageKind::Terms, PageKind::About]
    );
}

#[test]
fn links_to_other_sites_are_ignored_but_subdomains_of_the_domain_are_kept() {
    let html = r#"
        <a href="https://linkedin.com/company/acme/about">LinkedIn</a>
        <a href="https://evil.example/mentions-legales">Evil</a>
        <a href="https://blog.acme.fr/a-propos">Blog</a>
    "#;
    let links = page_links(html, &home(), &domain());
    assert_eq!(urls(&links), ["https://blog.acme.fr/a-propos"]);
}

#[test]
fn duplicate_links_and_the_home_page_are_dropped() {
    let html = r#"
        <a href="/">Home</a>
        <a href="/a-propos">A</a><a href="/a-propos#team">B</a><a href="/a-propos?x=1">C</a>
    "#;
    let links = page_links(html, &home(), &domain());
    assert_eq!(urls(&links), ["https://www.acme.fr/a-propos"]);
}

#[test]
fn the_fallback_paths_are_tried_only_when_no_link_of_the_kind_exists() {
    let none = candidates(&[], &home(), 10);
    assert_eq!(
        none.iter().map(|(url, _)| url.path()).collect::<Vec<_>>(),
        ["/mentions-legales", "/legal", "/a-propos", "/about"]
    );
    let html = r#"<a href="/mentions">M</a>"#;
    let links = page_links(html, &home(), &domain());
    let found = candidates(&links, &home(), 10);
    assert_eq!(
        found.iter().map(|(url, _)| url.path()).collect::<Vec<_>>(),
        ["/mentions", "/a-propos", "/about"]
    );
}

#[test]
fn the_candidate_list_is_capped() {
    assert_eq!(candidates(&[], &home(), 2).len(), 2);
    assert!(candidates(&[], &home(), 0).is_empty());
}

#[test]
fn a_labelled_siret_gives_its_siren() {
    let text = "Société ACME SAS au capital de 10 000 euros. SIRET : 123 456 782 00019 - Paris.";
    assert_eq!(find_siren(text).unwrap().as_str(), "123456782");
}

#[test]
fn a_labelled_siren_and_an_rcs_mention_are_found() {
    assert_eq!(
        find_siren("N° SIREN 123456782").unwrap().as_str(),
        "123456782"
    );
    assert_eq!(
        find_siren("RCS Nanterre B 123 456 782").unwrap().as_str(),
        "123456782"
    );
    assert_eq!(
        find_siren("immatriculée sous le numéro 123.456.782")
            .unwrap()
            .as_str(),
        "123456782"
    );
}

#[test]
fn a_number_that_fails_the_check_digit_or_has_no_label_is_not_a_siren() {
    assert_eq!(find_siren("SIRET : 123 456 789 00019"), None);
    assert_eq!(find_siren("Téléphone 123 456 782"), None);
    assert_eq!(find_siren("capital 123456782 euros"), None);
}

#[test]
fn a_longer_number_is_not_cut_into_a_siren() {
    assert_eq!(find_siren("SIRET 1234567821234567"), None);
}

#[test]
fn the_first_valid_candidate_wins_over_an_invalid_one() {
    let text = "SIRET 123 456 789 00019 puis SIREN 123 456 782";
    assert_eq!(find_siren(text).unwrap().as_str(), "123456782");
}

fn siren() -> Siren {
    Siren::parse("552081317").unwrap()
}

const EDF: &str = r#"{"results":[{"siren":"552081317","nom_complet":"ELECTRICITE DE FRANCE (EDF)","nom_raison_sociale":"ELECTRICITE DE FRANCE","activite_principale":"35.11Z","section_activite_principale":"D","tranche_effectif_salarie":"53","date_creation":"1955-01-01","siege":{"code_postal":"75008","libelle_commune":"PARIS","libelle_pays_etranger":null,"tranche_effectif_salarie":"32"}}]}"#;

#[test]
fn the_official_answer_is_mapped() {
    let official = parse_official(EDF.as_bytes(), &siren()).unwrap().unwrap();
    assert_eq!(official.legal_name, "ELECTRICITE DE FRANCE (EDF)");
    assert_eq!(official.naf_code.as_deref(), Some("35.11Z"));
    assert!(
        official
            .naf_label
            .unwrap()
            .starts_with("Production et distribution")
    );
    assert_eq!(official.headcount_code.as_deref(), Some("53"));
    assert_eq!(
        official.headcount_range.as_deref(),
        Some("10 000 salariés et plus")
    );
    assert_eq!(official.head_office_city.as_deref(), Some("PARIS"));
    assert_eq!(official.head_office_postcode.as_deref(), Some("75008"));
    assert_eq!(official.head_office_country.as_deref(), Some("France"));
    assert_eq!(official.head_office_country_code.as_deref(), Some("FR"));
    assert_eq!(
        official.created_on,
        chrono::NaiveDate::from_ymd_opt(1955, 1, 1)
    );
}

#[test]
fn a_foreign_head_office_keeps_its_name_without_a_code() {
    let body = r#"{"results":[{"siren":"552081317","nom_complet":"ACME","siege":{"libelle_pays_etranger":"ALLEMAGNE"}}]}"#;
    let official = parse_official(body.as_bytes(), &siren()).unwrap().unwrap();
    assert_eq!(official.head_office_country.as_deref(), Some("ALLEMAGNE"));
    assert_eq!(official.head_office_country_code, None);
}

#[test]
fn an_answer_for_another_siren_or_without_results_is_not_found() {
    assert_eq!(parse_official(br#"{"results":[]}"#, &siren()), Ok(None));
    let other = EDF.replace("552081317", "123456782");
    assert_eq!(parse_official(other.as_bytes(), &siren()), Ok(None));
}

#[test]
fn a_malformed_answer_is_an_error() {
    assert!(parse_official(b"<html>", &siren()).is_err());
}
