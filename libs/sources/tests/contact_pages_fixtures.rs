use std::{fs, path::PathBuf};

use sources::contact_pages::extract::extract_addresses;

fn extracted(name: &str) -> Vec<(String, f64)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contact_pages")
        .join(name);
    let html = fs::read_to_string(path).unwrap();
    let mut found: Vec<(String, f64)> = extract_addresses(&html)
        .into_iter()
        .map(|item| (item.address.to_string(), item.confidence))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn expected(items: &[(&str, f64)]) -> Vec<(String, f64)> {
    items
        .iter()
        .map(|(address, confidence)| ((*address).to_owned(), *confidence))
        .collect()
}

#[test]
fn french_legal_notice_yields_obfuscated_and_dpo_addresses() {
    assert_eq!(
        extracted("mentions-legales-acme-industrie.html"),
        expected(&[
            ("contact@acme-industrie.fr", 0.6),
            ("marie.durand@acme-industrie.fr", 0.9),
        ])
    );
}

#[test]
fn english_contact_page_yields_mailto_and_entity_encoded_addresses() {
    assert_eq!(
        extracted("contact-northwind-tools.html"),
        expected(&[
            ("jane.doe@northwind-tools.dev", 0.8),
            ("press@northwind-tools.dev", 0.9),
            ("sales@northwind-tools.dev", 0.9),
            ("support@northwind-tools.dev", 0.9),
        ])
    );
}

#[test]
fn cloudflare_protected_page_is_decoded() {
    assert_eq!(
        extracted("contact-bluefjord-labs.html"),
        expected(&[
            ("contact@bluefjord-labs.io", 0.8),
            ("legal@bluefjord-labs.io", 0.8),
        ])
    );
}

#[test]
fn addresses_only_in_scripts_yield_nothing() {
    assert_eq!(extracted("scripts-only-quartzwave.html"), expected(&[]));
}

#[test]
fn footer_only_homepage_yields_footer_addresses() {
    assert_eq!(
        extracted("homepage-footer-lumenpress.html"),
        expected(&[("accueil@lumenpress.fr", 0.6), ("devis@lumenpress.fr", 0.6),])
    );
}
