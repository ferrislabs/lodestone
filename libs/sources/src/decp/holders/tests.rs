use chrono::TimeZone;
use serde_json::{Value, to_vec};

use super::*;

const HOLDER: &str = include_str!("../../../tests/fixtures/decp/titulaire-552100554.json");
const SOURCE: &str = "https://tabular.test/data/?titulaire_id__contains=552100554";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 8, 9, 0, 0).unwrap()
}

fn siren() -> Siren {
    Siren::parse("552100554").unwrap()
}

#[test]
fn awards_held_by_the_siren_or_one_of_its_sirets_are_kept_most_recent_first() {
    let findings = parse_awards(HOLDER.as_bytes(), &siren(), SOURCE, now()).unwrap();
    let rows: Vec<(&str, &str, &str)> = findings
        .iter()
        .map(|found| {
            (
                found.details["date"].as_str().unwrap(),
                found.details["holder_siret"].as_str().unwrap(),
                found.value.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "2026-10-07",
                "55210055400013",
                "Diagnostic des réseaux d'adduction d'eau potable"
            ),
            (
                "2026-08-10",
                "55210055400047",
                "Assainissement collectif : diagnostic des réseaux"
            ),
            (
                "2025-03-01",
                "552100554",
                "Maintenance du logiciel de gestion"
            ),
        ]
    );
    assert!(findings.iter().all(|found| found.kind == "contract"
        && found.label == "Marché remporté"
        && found.module == EcosystemModule::PublicContracts
        && found.source_url == SOURCE));
}

#[test]
fn an_award_carries_its_buyer_amount_and_procedure() {
    let findings = parse_awards(HOLDER.as_bytes(), &siren(), SOURCE, now()).unwrap();
    let iroise = &findings[1];
    assert_eq!(
        iroise.details["buyer"],
        "COMMUNAUTE DE COMMUNES DU PAYS D IROISE"
    );
    assert_eq!(iroise.details["buyer_siret"], "24290007400251");
    assert_eq!(iroise.details["amount_eur"], 400000.0);
    assert_eq!(iroise.details["procedure"], "Procédure avec négociation");
    assert_eq!(iroise.key, "contract:24290007400251A25052S");
    assert_eq!(
        iroise.observed_at,
        Utc.with_ymd_and_hms(2026, 8, 10, 0, 0, 0).unwrap()
    );
    assert_eq!(findings[0].details["buyer"], Value::Null);
    assert_eq!(findings[2].details["amount_eur"], 12500.0);
}

#[test]
fn awards_are_capped() {
    let data: Vec<Value> = (0..60)
        .map(|index| {
            json!({
                "uid": format!("U{index}"),
                "titulaire_id": "55210055400047",
                "titulaire_typeIdentifiant": "SIRET",
                "objet": format!("Marché {index}"),
                "dateNotification": format!("2026-{:02}-01", index % 12 + 1),
            })
        })
        .collect();
    let body = to_vec(&json!({"data": data})).unwrap();
    let findings = parse_awards(&body, &siren(), SOURCE, now()).unwrap();
    assert_eq!(findings.len(), MAX_PUBLIC_CONTRACTS);
    assert_eq!(findings[0].details["date"], "2026-12-01");
}

#[test]
fn a_malformed_answer_is_an_error() {
    assert!(parse_awards(b"{\"data\": 3}", &siren(), SOURCE, now()).is_err());
}

#[test]
fn the_query_looks_for_the_siren_in_the_holder_column() {
    let base = Url::parse(DECP_HOLDERS_URL).unwrap();
    let url = holder_query(&base, &siren());
    assert_eq!(url.host_str(), Some("tabular-api.data.gouv.fr"));
    let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        [
            ("titulaire_id__contains".to_owned(), "552100554".to_owned()),
            ("dateNotification__sort".to_owned(), "desc".to_owned()),
            ("page_size".to_owned(), "50".to_owned()),
        ]
    );
}
