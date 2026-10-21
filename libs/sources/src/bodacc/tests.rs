use chrono::TimeZone;
use serde_json::to_vec;

use super::*;

const ANNOUNCEMENTS: &str = include_str!("../../tests/fixtures/bodacc/annonces-552100554.json");
const SOURCE: &str = "https://bodacc.test/records?where=registre%3D%22552100554%22";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
}

fn parsed() -> Vec<EcosystemFinding> {
    parse_announcements(ANNOUNCEMENTS.as_bytes(), SOURCE, now()).unwrap()
}

#[test]
fn every_announcement_type_gives_a_dated_finding_most_recent_first() {
    let findings = parsed();
    let rows: Vec<(&str, &str, &str)> = findings
        .iter()
        .map(|found| {
            (
                found.kind.as_str(),
                found.label.as_str(),
                found.details["date"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "collective_proceeding",
                "Procédure collective",
                "2026-09-10"
            ),
            ("accounts_filing", "Dépôt des comptes", "2026-06-02"),
            ("modification", "Modification", "2026-03-08"),
            ("sale", "Vente ou cession", "2020-11-24"),
            ("deregistration", "Radiation", "2019-02-04"),
            ("creation", "Création", "2004-09-02"),
        ]
    );
    assert!(
        findings
            .iter()
            .all(|found| found.module == EcosystemModule::LegalLife)
    );
    assert!(findings.iter().all(|found| found.source_url == SOURCE));
}

#[test]
fn a_collective_proceeding_keeps_its_nature_court_and_link() {
    let findings = parsed();
    let collective = &findings[0];
    assert_eq!(
        collective.value,
        "Jugement d'ouverture d'une procédure de redressement judiciaire"
    );
    assert_eq!(collective.key, "bodacc:A202601733059");
    assert_eq!(collective.details["collective"], true);
    assert_eq!(collective.details["judgment_date"], "2026-09-01");
    assert_eq!(
        collective.details["court"],
        "Greffe du Tribunal des Activités Economiques de Lyon"
    );
    assert_eq!(
        collective.details["link"],
        "https://www.bodacc.fr/pages/annonces-commerciales-detail/?q.id=id:A202601733059"
    );
    assert_eq!(
        collective.observed_at,
        Utc.with_ymd_and_hms(2026, 9, 10, 0, 0, 0).unwrap()
    );
}

#[test]
fn each_type_gets_a_short_description() {
    let findings = parsed();
    let values: Vec<&str> = findings.iter().map(|found| found.value.as_str()).collect();
    assert_eq!(
        values[1..],
        [
            "Comptes annuels et rapports, clôture le 2025-12-31",
            "Modification de l'administration. Transfert du siège social.",
            "Achat d'un fonds par une personne morale (avis rectificatif)",
            "Radiation du registre du commerce et des sociétés",
            "Création d'un établissement principal",
        ]
    );
    assert_eq!(findings[4].details["link"], Value::Null);
    assert_eq!(findings[1].details["collective"], false);
}

#[test]
fn announcements_are_capped() {
    let results: Vec<Value> = (0..30)
        .map(|day| {
            json!({
                "id": format!("C{day}"),
                "dateparution": format!("2026-01-{:02}", day % 28 + 1),
                "familleavis": "dpc",
                "depot": {"typeDepot": "Comptes annuels"}
            })
        })
        .collect();
    let body = to_vec(&json!({"results": results})).unwrap();
    let findings = parse_announcements(&body, SOURCE, now()).unwrap();
    assert_eq!(findings.len(), MAX_LEGAL_EVENTS);
    assert_eq!(findings[0].details["date"], "2026-01-28");
}

#[test]
fn a_malformed_answer_is_an_error() {
    assert!(parse_announcements(b"<html>", SOURCE, now()).is_err());
}

#[test]
fn the_records_query_filters_by_siren_most_recent_first() {
    let dataset = Url::parse(
        "https://bodacc-datadila.opendatasoft.com/api/explore/v2.1/catalog/datasets/annonces-commerciales",
    )
    .unwrap();
    let url = records_url(&dataset, &Siren::parse("552100554").unwrap()).unwrap();
    assert_eq!(
        url.path(),
        "/api/explore/v2.1/catalog/datasets/annonces-commerciales/records"
    );
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [
            ("where".to_owned(), "registre=\"552100554\"".to_owned()),
            ("order_by".to_owned(), "dateparution desc".to_owned()),
            ("limit".to_owned(), "20".to_owned()),
        ]
    );
}
