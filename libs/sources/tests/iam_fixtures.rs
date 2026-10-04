mod fixtures_support;

use std::collections::BTreeSet;

use fixtures_support::{all, files_on_disk, indexed_files};

#[test]
fn index_lists_exactly_the_fixture_files() {
    let indexed: BTreeSet<String> = indexed_files().into_iter().collect();
    assert_eq!(indexed, files_on_disk());
}

#[test]
fn every_fixture_is_well_formed() {
    let products = ["keycloak", "authentik", "zitadel", "ferriskey", "none"];
    for fixture in all() {
        assert!(
            products.contains(&fixture.product.as_str()),
            "{}",
            fixture.file
        );
        assert!(fixture.path.starts_with('/'), "{}", fixture.file);
        assert!((100..600).contains(&fixture.status), "{}", fixture.file);
        assert!(
            fixture
                .headers
                .iter()
                .all(|(name, _)| name == &name.to_ascii_lowercase()),
            "{}",
            fixture.file
        );
    }
}

#[test]
fn fixtures_carry_no_capture_host_or_session_data() {
    for fixture in all() {
        let dropped = ["date", "set-cookie", "x-authentik-id"];
        assert!(
            fixture
                .headers
                .iter()
                .all(|(name, _)| !dropped.contains(&name.as_str())),
            "{}",
            fixture.file
        );
        for leak in ["127.0.0.1:", "localhost:"] {
            assert!(
                !fixture.body.contains(leak),
                "{} contains {leak}",
                fixture.file
            );
        }
    }
}

#[test]
fn corpus_covers_every_product() {
    let products: BTreeSet<String> = all().into_iter().map(|fixture| fixture.product).collect();
    for product in ["keycloak", "authentik", "zitadel", "ferriskey", "none"] {
        assert!(products.contains(product), "missing {product}");
    }
}
