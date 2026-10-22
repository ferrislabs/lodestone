use serde_json::Value;
use url::Url;

use crate::domain::{lead::Signal, values::SourceName};

const SUBJECT_FALLBACK: &str = "Un hôte du domaine";

pub(super) fn host_candidates(signal: &Signal) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(host) = signal.raw.get("host").and_then(Value::as_str) {
        candidates.push(host.to_owned());
    }
    let provenance = signal.provenance.as_str();
    if let Some(rest) = provenance.strip_prefix("dns:") {
        let name = rest.split('?').next().unwrap_or_default();
        if !name.is_empty() {
            candidates.push(name.to_owned());
        }
    } else if let Some(host) = Url::parse(provenance)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    {
        candidates.push(host);
    }
    candidates
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn text_at<'a>(raw: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = raw;
    for key in path {
        current = current.get(key)?;
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn keywords(raw: &Value) -> Option<String> {
    let matched: Vec<&str> = raw
        .get("matched")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    (!matched.is_empty()).then(|| format!("mots-clés : {}", matched.join(", ")))
}

fn with_details(base: String, details: Vec<String>) -> String {
    if details.is_empty() {
        base
    } else {
        format!("{base} ({})", details.join(" ; "))
    }
}

fn address(signal: &Signal) -> Option<&str> {
    text_at(&signal.raw, &["address"]).or(signal.value.as_deref())
}

pub fn signal_sentence(signal: &Signal) -> String {
    let kind = signal.kind.as_str();
    let value = signal
        .value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let host = host_candidates(signal).into_iter().next();
    let subject = host.as_deref().unwrap_or(SUBJECT_FALLBACK);
    if signal.source == SourceName::CertificateTransparency {
        let name = text_at(&signal.raw, &["host"])
            .or(value)
            .unwrap_or(SUBJECT_FALLBACK);
        return format!("{name} apparaît dans les certificats publics (Certificate Transparency)");
    }
    match (kind, value) {
        ("saas_provider", Some(provider)) => {
            format!("{subject} pointe vers {} (CNAME)", capitalised(provider))
        }
        ("product_detected", Some(product)) => {
            let details = match text_at(&signal.raw, &["version"]) {
                Some(version) => format!("sonde HTTP, version {version}"),
                None => "sonde HTTP".to_owned(),
            };
            format!("{subject} expose {} ({details})", capitalised(product))
        }
        ("debt_marker", Some(marker)) => {
            format!("{subject} présente un marqueur de dette technique : {marker} (sonde HTTP)")
        }
        ("role_contact_found", _) => match address(signal) {
            Some(found) => format!("Adresse de contact trouvée sur une page publique : {found}"),
            None => fallback(kind, value),
        },
        ("security_txt_contact", _) => match address(signal) {
            Some(found) => format!("Contact de sécurité publié dans security.txt : {found}"),
            None => fallback(kind, value),
        },
        ("role_contact_guessed", _) => match address(signal) {
            Some(found) => format!("Adresse de rôle supposée, non vérifiée : {found}"),
            None => fallback(kind, value),
        },
        ("public_tender", Some(title)) => {
            let mut details = Vec::new();
            if let Some(buyer) = text_at(&signal.raw, &["details", "buyer"]) {
                details.push(format!("acheteur : {buyer}"));
            }
            details.extend(keywords(&signal.raw));
            with_details(format!("Appel d'offres public : {title}"), details)
        }
        ("awarded_contract", Some(title)) => {
            let mut details = Vec::new();
            if let Some(buyer) = text_at(&signal.raw, &["details", "buyer_name"]) {
                details.push(format!("acheteur : {buyer}"));
            }
            details.extend(keywords(&signal.raw));
            with_details(format!("Marché public attribué : {title}"), details)
        }
        ("job_offer", Some(title)) => {
            let mut details = Vec::new();
            if let Some(company) = text_at(&signal.raw, &["details", "company"]) {
                details.push(format!("entreprise : {company}"));
            }
            details.extend(keywords(&signal.raw));
            with_details(format!("Offre d'emploi : {title}"), details)
        }
        _ => fallback(kind, value),
    }
}

pub fn signal_excerpts(signal: &Signal) -> Vec<&str> {
    signal
        .raw
        .get("excerpts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|passage| !passage.is_empty())
        .collect()
}

fn fallback(kind: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{kind} : {value}"),
        None => kind.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use serde_json::json;

    use super::*;
    use crate::domain::{
        lead::{LeadId, SignalId},
        values::{Provenance, SignalKind, UnitInterval},
    };

    fn signal(
        kind: &str,
        value: Option<&str>,
        source: SourceName,
        raw: Value,
        provenance: &str,
    ) -> Signal {
        Signal {
            id: SignalId(uuid::Uuid::nil()),
            lead_id: LeadId(uuid::Uuid::nil()),
            kind: SignalKind::parse(kind).unwrap(),
            value: value.map(str::to_owned),
            source,
            raw,
            provenance: Provenance::parse(provenance).unwrap(),
            strength: UnitInterval::new(0.9).unwrap(),
            observed_at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            agent: None,
        }
    }

    #[test]
    fn a_cname_names_the_host_and_the_provider() {
        let signal = signal(
            "saas_provider",
            Some("auth0"),
            SourceName::DnsCname,
            json!({"host": "login.dev.luzernrisk.com", "chain": [], "matched": "x.auth0.com"}),
            "dns:login.dev.luzernrisk.com?type=CNAME",
        );
        assert_eq!(
            signal_sentence(&signal),
            "login.dev.luzernrisk.com pointe vers Auth0 (CNAME)"
        );
    }

    #[test]
    fn a_cname_without_a_stored_host_reads_it_from_the_provenance() {
        let signal = signal(
            "saas_provider",
            Some("okta"),
            SourceName::DnsCname,
            json!({}),
            "dns:sso.acme.com?type=CNAME",
        );
        assert_eq!(
            signal_sentence(&signal),
            "sso.acme.com pointe vers Okta (CNAME)"
        );
    }

    #[test]
    fn a_product_names_the_host_the_probe_and_the_version() {
        let with_version = signal(
            "product_detected",
            Some("keycloak"),
            SourceName::HttpProbe,
            json!({"host": "sso.acme.com", "version": "21.1"}),
            "https://sso.acme.com/realms/master",
        );
        assert_eq!(
            signal_sentence(&with_version),
            "sso.acme.com expose Keycloak (sonde HTTP, version 21.1)"
        );
        let without = signal(
            "product_detected",
            Some("keycloak"),
            SourceName::HttpProbe,
            json!({"host": "sso.acme.com", "version": null}),
            "https://sso.acme.com/realms/master",
        );
        assert_eq!(
            signal_sentence(&without),
            "sso.acme.com expose Keycloak (sonde HTTP)"
        );
    }

    #[test]
    fn a_probe_without_host_in_raw_reads_the_provenance_url() {
        let signal = signal(
            "product_detected",
            Some("keycloak"),
            SourceName::HttpProbe,
            json!({}),
            "https://sso.acme.com/realms/master",
        );
        assert!(signal_sentence(&signal).starts_with("sso.acme.com expose"));
    }

    #[test]
    fn a_debt_marker_is_a_technical_debt_sentence() {
        let signal = signal(
            "debt_marker",
            Some("outdated_version"),
            SourceName::HttpProbe,
            json!({"host": "sso.acme.com"}),
            "https://sso.acme.com/realms/master",
        );
        assert_eq!(
            signal_sentence(&signal),
            "sso.acme.com présente un marqueur de dette technique : outdated_version (sonde HTTP)"
        );
    }

    #[test]
    fn a_certificate_transparency_signal_names_the_host() {
        let signal = signal(
            "host_seen",
            Some("login.acme.com"),
            SourceName::CertificateTransparency,
            json!({}),
            "https://crt.sh/?q=login.acme.com",
        );
        assert_eq!(
            signal_sentence(&signal),
            "login.acme.com apparaît dans les certificats publics (Certificate Transparency)"
        );
        let anonymous = Signal {
            value: None,
            ..signal
        };
        assert_eq!(
            signal_sentence(&anonymous),
            "Un hôte du domaine apparaît dans les certificats publics (Certificate Transparency)"
        );
    }

    #[test]
    fn contacts_are_told_apart_by_how_they_were_found() {
        let found = signal(
            "role_contact_found",
            Some("security"),
            SourceName::PublicPage,
            json!({"address": "security@acme.com", "method": "public_page"}),
            "https://acme.com/contact",
        );
        assert_eq!(
            signal_sentence(&found),
            "Adresse de contact trouvée sur une page publique : security@acme.com"
        );
        let txt = signal(
            "security_txt_contact",
            Some("security"),
            SourceName::SecurityTxt,
            json!({"address": "security@acme.com"}),
            "https://acme.com/.well-known/security.txt",
        );
        assert_eq!(
            signal_sentence(&txt),
            "Contact de sécurité publié dans security.txt : security@acme.com"
        );
        let guessed = signal(
            "role_contact_guessed",
            Some("it"),
            SourceName::RoleGuess,
            json!({}),
            "https://acme.com/",
        );
        assert_eq!(
            signal_sentence(&guessed),
            "Adresse de rôle supposée, non vérifiée : it"
        );
    }

    #[test]
    fn a_tender_names_the_buyer_and_the_keywords() {
        let signal = signal(
            "public_tender",
            Some("Refonte de l'authentification"),
            SourceName::Boamp,
            json!({"title": "x", "matched": ["keycloak", "iam"], "details": {"buyer": "Mairie de Lyon"}}),
            "https://www.boamp.fr/avis/1",
        );
        assert_eq!(
            signal_sentence(&signal),
            "Appel d'offres public : Refonte de l'authentification (acheteur : Mairie de Lyon ; mots-clés : keycloak, iam)"
        );
    }

    #[test]
    fn a_tender_without_details_is_just_the_title() {
        let signal = signal(
            "public_tender",
            Some("Marché IAM"),
            SourceName::Ted,
            json!({"matched": []}),
            "https://ted.europa.eu/notice/1",
        );
        assert_eq!(
            signal_sentence(&signal),
            "Appel d'offres public : Marché IAM"
        );
    }

    #[test]
    fn an_awarded_contract_names_the_buyer() {
        let signal = signal(
            "awarded_contract",
            Some("Plateforme IAM"),
            SourceName::Decp,
            json!({"details": {"buyer_name": "Région Sud"}}),
            "https://decp.example/1",
        );
        assert_eq!(
            signal_sentence(&signal),
            "Marché public attribué : Plateforme IAM (acheteur : Région Sud)"
        );
    }

    #[test]
    fn a_job_offer_names_the_company() {
        let signal = signal(
            "job_offer",
            Some("Ingénieur IAM"),
            SourceName::FranceTravail,
            json!({"matched": ["iam"], "details": {"company": "Acme"}}),
            "https://candidat.francetravail.fr/offres/1",
        );
        assert_eq!(
            signal_sentence(&signal),
            "Offre d'emploi : Ingénieur IAM (entreprise : Acme ; mots-clés : iam)"
        );
    }

    #[test]
    fn an_unknown_kind_falls_back_to_kind_and_value() {
        let with_value = signal(
            "custom_kind",
            Some("x"),
            SourceName::Ats,
            json!({}),
            "https://acme.com/",
        );
        assert_eq!(signal_sentence(&with_value), "custom_kind : x");
        let without = Signal {
            value: None,
            ..with_value
        };
        assert_eq!(signal_sentence(&without), "custom_kind");
    }

    #[test]
    fn a_known_kind_without_a_value_falls_back_too() {
        let signal = signal(
            "saas_provider",
            None,
            SourceName::DnsCname,
            json!({}),
            "dns:sso.acme.com?type=CNAME",
        );
        assert_eq!(signal_sentence(&signal), "saas_provider");
    }

    #[test]
    fn excerpts_are_read_in_order_from_the_raw_signal() {
        let signal = signal(
            "job_offer",
            Some("Ingénieur IAM"),
            SourceName::FranceTravail,
            json!({"matched": ["iam"], "excerpts": ["…migration vers Keycloak…", "projet IAM"]}),
            "https://candidat.francetravail.fr/offres/1",
        );
        assert_eq!(
            signal_excerpts(&signal),
            ["…migration vers Keycloak…", "projet IAM"]
        );
    }

    #[test]
    fn excerpts_skip_non_strings_and_blanks() {
        let signal = signal(
            "public_tender",
            Some("Marché IAM"),
            SourceName::Boamp,
            json!({"excerpts": ["un", 3, null, "  ", {"a": 1}, " deux "]}),
            "https://www.boamp.fr/avis/1",
        );
        assert_eq!(signal_excerpts(&signal), ["un", "deux"]);
    }

    #[test]
    fn a_signal_without_usable_excerpts_has_none() {
        for raw in [
            json!({}),
            json!({"excerpts": []}),
            json!({"excerpts": "texte"}),
            json!({"excerpts": {"a": "b"}}),
            json!({"excerpts": null}),
            json!("excerpts"),
        ] {
            let signal = signal(
                "public_tender",
                Some("Marché IAM"),
                SourceName::Boamp,
                raw,
                "https://www.boamp.fr/avis/1",
            );
            assert!(signal_excerpts(&signal).is_empty());
        }
    }

    #[test]
    fn the_sentence_never_leaks_the_raw_json() {
        let signal = signal(
            "saas_provider",
            Some("auth0"),
            SourceName::DnsCname,
            json!({"host": "a.acme.com", "chain": ["b.auth0.com"]}),
            "dns:a.acme.com?type=CNAME",
        );
        assert!(!signal_sentence(&signal).contains('{'));
    }
}
