//! Benchmarks for the pure domain logic of `lodestone-core`: domain
//! normalisation (called for every observed host), profile validation,
//! keyword triage and lead scoring.

use chrono::{TimeZone, Utc};
use lodestone_core::{
    DISTINCT_SIGNALS_FACTOR, LeadId, NormalizedDomain, Provenance, Signal, SignalId, SignalKind,
    SourceName, UnitInterval, ValidatedProfile, definition::ProfileDefinition, market_context,
    route, score,
};
use uuid::Uuid;

const IAM_PROFILE: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");
const ECOMMERCE_PROFILE: &str =
    include_str!("../../../docs/examples/profiles/ecommerce-replatforming.json");

fn main() {
    divan::main();
}

fn profile(raw: &str) -> ProfileDefinition {
    let value: serde_json::Value = serde_json::from_str(raw).unwrap();
    ValidatedProfile::parse(value).unwrap().definition().clone()
}

mod normalized_domain {
    use super::*;

    const HOSTS: &[&str] = &[
        "sso.acme.com",
        "*.auth.example.co.uk",
        "login.intranet.mairie-de-paris.fr.",
        "keycloak.prod.eu-west-1.internal.bigcorp.io",
        "münchen.example.de",
        "api.service.gov.uk",
    ];

    const URLS: &[&str] = &[
        "https://sso.acme.com/realms/master/protocol/openid-connect/auth",
        "http://www.example.co.uk/careers?ref=lodestone",
        "https://jobs.lever.co/qonto/1234-5678",
        "https://boamp.fr/pages/avis/?q=idweb:26-12345",
    ];

    #[divan::bench]
    fn from_host() -> usize {
        HOSTS
            .iter()
            .filter_map(|host| NormalizedDomain::from_host(divan::black_box(host)).ok())
            .count()
    }

    #[divan::bench]
    fn from_url() -> usize {
        URLS.iter()
            .filter_map(|url| NormalizedDomain::from_url(divan::black_box(url)).ok())
            .count()
    }
}

mod profile_definition {
    use super::*;

    #[divan::bench(args = ["iam", "ecommerce"])]
    fn validate(bencher: divan::Bencher, name: &str) {
        let raw = match name {
            "iam" => IAM_PROFILE,
            _ => ECOMMERCE_PROFILE,
        };
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        bencher
            .with_inputs(|| value.clone())
            .bench_local_values(|value| divan::black_box(ValidatedProfile::parse(value).unwrap()));
    }

    #[divan::bench]
    fn market_context_iam(bencher: divan::Bencher) {
        let definition = profile(IAM_PROFILE);
        bencher.bench_local(|| divan::black_box(market_context(divan::black_box(&definition))));
    }
}

mod triage {
    use super::*;

    const OFFER: &str = "Nous recherchons un ingénieur IAM pour reprendre la gestion des \
        identités de notre SI : migration de l'annuaire LDAP vers Keycloak, mise en place du \
        SSO OpenID Connect pour l'ensemble des applications métiers, revue des habilitations \
        et accompagnement des équipes. Expérience sur des projets de fédération d'identités \
        (SAML, OIDC) et de MFA appréciée. Poste basé à Lyon, télétravail partiel possible.";

    #[divan::bench]
    fn route_job_offer(bencher: divan::Bencher) {
        let definition = profile(IAM_PROFILE);
        let rules = definition.sources.france_travail.clone().unwrap();
        bencher.bench_local(|| divan::black_box(route(divan::black_box(OFFER), &rules)));
    }
}

mod scoring {
    use super::*;

    const KINDS: &[(&str, Option<&str>)] = &[
        ("product_detected", Some("keycloak")),
        ("debt_marker", Some("legacy_auth_path")),
        ("saas_provider", Some("auth0")),
        ("job_offer", None),
        ("public_tender", None),
        ("role_contact_found", None),
    ];

    fn signals(count: usize) -> Vec<Signal> {
        let observed_at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        (0..count)
            .map(|i| {
                let (kind, value) = KINDS[i % KINDS.len()];
                Signal {
                    id: SignalId(Uuid::nil()),
                    lead_id: LeadId(Uuid::nil()),
                    kind: SignalKind::parse(kind).unwrap(),
                    value: value.map(str::to_owned),
                    source: SourceName::HttpProbe,
                    raw: serde_json::Value::Null,
                    provenance: Provenance::parse("https://sso.acme.com/").unwrap(),
                    strength: UnitInterval::new((i % 10) as f64 / 10.0).unwrap(),
                    observed_at,
                }
            })
            .collect()
    }

    #[divan::bench(args = [6, 60, 600])]
    fn score_lead(bencher: divan::Bencher, count: usize) {
        let definition = profile(IAM_PROFILE);
        assert!(
            definition
                .scoring
                .factors
                .iter()
                .any(|factor| factor.signal == DISTINCT_SIGNALS_FACTOR)
        );
        let signals = signals(count);
        bencher.bench_local(|| {
            divan::black_box(score(divan::black_box(&signals), &definition.scoring))
        });
    }
}
