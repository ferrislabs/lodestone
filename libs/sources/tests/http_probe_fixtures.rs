use std::{path::PathBuf, sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, CandidateHost, CandidateHosts, FetchError, FetchRequest, FetchResponse, Fetcher,
    HostOrigin, Hostname, Observation, Profile, ProfileId, ProfileName, ProfileRevision,
    RevisionNumber, Source, SourceName, ValidatedProfile,
};
use sources::http_probe::{
    HttpProbeSettings, HttpProbeSource, ProbeGuard, ProbeScheme, ProbedHost, ProbedPath,
    ProbedResponse,
};
use uuid::Uuid;

mod fixtures_support;

const AUTHENTIK: &[&str] = &[
    "authentik/ak---health-live-slash.json",
    "authentik/ak---health-ready-slash.json",
    "authentik/ak-api-v3-admin-version-slash.json",
    "authentik/ak-api-v3-root-config-slash.json",
    "authentik/ak-api-v3-slash.json",
    "authentik/ak-application-o--well-known-openid-configuration.json",
    "authentik/ak-application-o-slash.json",
    "authentik/ak-if-flow-initial-setup-slash.json",
    "authentik/ak-realms-master.json",
    "authentik/ak-root.json",
    "authentik/ak-setup.json",
    "authentik/ak-static-dist-custom-css.json",
];

const FERRISKEY: &[&str] = &[
    "ferriskey/fk-auth-realms-master.json",
    "ferriskey/fk-health.json",
    "ferriskey/fk-master-certs.json",
    "ferriskey/fk-master-openid-configuration.json",
    "ferriskey/fk-openapi.json",
    "ferriskey/fk-realm-nonexistent.json",
    "ferriskey/fk-realms-master.json",
    "ferriskey/fk-root.json",
];

const KC17_LEGACY: &[&str] = &[
    "keycloak/kc17-legacy-auth-health.json",
    "keycloak/kc17-legacy-auth-realms-master--well-known-openid-configuration.json",
    "keycloak/kc17-legacy-auth-realms-master-protocol-openid-connect-certs.json",
    "keycloak/kc17-legacy-auth-realms-master.json",
    "keycloak/kc17-legacy-auth-slash.json",
    "keycloak/kc17-legacy-health-ready.json",
    "keycloak/kc17-legacy-health.json",
    "keycloak/kc17-legacy-realms-master--well-known-openid-configuration.json",
    "keycloak/kc17-legacy-realms-master.json",
    "keycloak/kc17-legacy-root.json",
];

const KC24: &[&str] = &[
    "keycloak/kc24-auth-health.json",
    "keycloak/kc24-auth-realms-master--well-known-openid-configuration.json",
    "keycloak/kc24-auth-realms-master.json",
    "keycloak/kc24-auth-slash.json",
    "keycloak/kc24-health-ready.json",
    "keycloak/kc24-health.json",
    "keycloak/kc24-realms-master--well-known-openid-configuration.json",
    "keycloak/kc24-realms-master.json",
    "keycloak/kc24-root.json",
];

const KC26: &[&str] = &[
    "keycloak/kc26-auth-health.json",
    "keycloak/kc26-auth-realms-master--well-known-openid-configuration.json",
    "keycloak/kc26-auth-realms-master.json",
    "keycloak/kc26-auth-slash.json",
    "keycloak/kc26-health-ready.json",
    "keycloak/kc26-health.json",
    "keycloak/kc26-realms-master--well-known-openid-configuration.json",
    "keycloak/kc26-realms-master.json",
    "keycloak/kc26-root.json",
];

const KC26_AUTHPREFIX: &[&str] = &[
    "keycloak/kc26-authprefix-auth-health.json",
    "keycloak/kc26-authprefix-auth-realms-master--well-known-openid-configuration.json",
    "keycloak/kc26-authprefix-auth-realms-master.json",
    "keycloak/kc26-authprefix-auth-slash.json",
    "keycloak/kc26-authprefix-health-ready.json",
    "keycloak/kc26-authprefix-health.json",
    "keycloak/kc26-authprefix-realms-master--well-known-openid-configuration.json",
    "keycloak/kc26-authprefix-realms-master.json",
    "keycloak/kc26-authprefix-root.json",
];

const HTTPD: &[&str] = &["none/httpd-realms-master.json", "none/httpd-root.json"];

const NGINX: &[&str] = &["none/nginx-realms-master.json", "none/nginx-root.json"];

const ZITADEL: &[&str] = &[
    "zitadel/zitadel-debug-healthz.json",
    "zitadel/zitadel-debug-ready.json",
    "zitadel/zitadel-debug-validate.json",
    "zitadel/zitadel-oauth-keys.json",
    "zitadel/zitadel-openid-configuration.json",
    "zitadel/zitadel-realms-master.json",
    "zitadel/zitadel-root.json",
    "zitadel/zitadel-ui-console.json",
];

struct NoFetch;

impl Fetcher for NoFetch {
    async fn fetch(&self, _request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        Err(FetchError::Timeout)
    }
}

struct NoHosts;

impl CandidateHosts for NoHosts {
    async fn upsert(&self, hosts: &[CandidateHost]) -> Result<u64, CoreError> {
        Ok(hosts.len() as u64)
    }

    async fn list_due(
        &self,
        _source: SourceName,
        _checked_before: DateTime<Utc>,
        _limit: u32,
    ) -> Result<Vec<CandidateHost>, CoreError> {
        Ok(Vec::new())
    }

    async fn mark_checked(
        &self,
        _hosts: &[Hostname],
        _source: SourceName,
        _at: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        Ok(())
    }
}

struct AllowAll;

impl ProbeGuard for AllowAll {
    async fn reserve(
        &self,
        _host: &Hostname,
        _now: DateTime<Utc>,
        _revisit: Duration,
    ) -> Result<bool, CoreError> {
        Ok(true)
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0).unwrap()
}

fn iam_profile() -> ActiveProfile {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/examples/profiles/iam-ferriskey.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    ActiveProfile {
        profile: Profile {
            id: ProfileId(Uuid::nil()),
            name: ProfileName::parse("iam").unwrap(),
            active: true,
            created_at: at(0),
            updated_at: at(0),
        },
        revision: ProfileRevision {
            profile_id: ProfileId(Uuid::nil()),
            number: RevisionNumber::FIRST,
            definition: ValidatedProfile::parse(value).unwrap(),
            created_by: None,
            created_at: at(0),
        },
    }
}

fn probe_paths(profile: &ActiveProfile) -> Vec<String> {
    let mut paths: Vec<String> = profile
        .revision
        .definition
        .definition()
        .enabled_agents(SourceName::HttpProbe)
        .next()
        .unwrap()
        .source
        .http_probe()
        .unwrap()
        .fingerprints
        .iter()
        .map(|fingerprint| fingerprint.path.clone())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn probed_host(files: &[&str], paths: &[String]) -> ProbedHost {
    let fixtures: Vec<_> = files
        .iter()
        .map(|file| fixtures_support::load(file))
        .collect();
    let responses = paths
        .iter()
        .map(|path| ProbedPath {
            path: path.clone(),
            response: fixtures
                .iter()
                .find(|fixture| &fixture.path == path)
                .map(|fixture| ProbedResponse {
                    status: fixture.status,
                    headers: fixture.headers.clone(),
                    body: fixture.body.clone(),
                }),
        })
        .collect();
    ProbedHost {
        host: CandidateHost::discovered(
            Hostname::parse("sso.example.com").unwrap(),
            HostOrigin::Import,
            at(0),
        )
        .unwrap(),
        responses,
        probed_at: at(5),
    }
}

fn observed(files: &[&str]) -> Vec<(String, String)> {
    let profile = iam_profile();
    let source = HttpProbeSource::new(
        Arc::new(NoFetch),
        Arc::new(NoHosts),
        Arc::new(AllowAll),
        HttpProbeSettings {
            batch_size: 10,
            concurrency: 1,
            revisit_interval: Duration::from_secs(60),
            scheme: ProbeScheme::Https,
            port: None,
            host_override: None,
        },
    );
    let item = probed_host(files, &probe_paths(&profile));
    let found: Vec<Observation> = source.evaluate(
        &item,
        &profile,
        profile
            .revision
            .definition
            .definition()
            .enabled_agents(SourceName::HttpProbe)
            .next()
            .unwrap(),
    );
    found
        .into_iter()
        .map(|observation| {
            (
                observation.kind.as_str().to_owned(),
                observation.value.unwrap(),
            )
        })
        .collect()
}

fn pair(kind: &str, value: &str) -> (String, String) {
    (kind.to_owned(), value.to_owned())
}

#[test]
fn every_fixture_belongs_to_a_known_deployment() {
    let known: usize = [
        AUTHENTIK,
        FERRISKEY,
        KC17_LEGACY,
        KC24,
        KC26,
        KC26_AUTHPREFIX,
        HTTPD,
        NGINX,
        ZITADEL,
    ]
    .iter()
    .map(|files| files.len())
    .sum();
    assert_eq!(known, fixtures_support::all().len());
}

#[test]
fn keycloak_current_is_detected_without_debt() {
    for files in [KC26, KC24] {
        assert_eq!(observed(files), vec![pair("product_detected", "keycloak")]);
    }
}

#[test]
fn keycloak_legacy_is_detected_with_legacy_auth_path() {
    assert_eq!(
        observed(KC17_LEGACY),
        vec![
            pair("product_detected", "keycloak"),
            pair("debt_marker", "legacy_auth_path")
        ]
    );
}

#[test]
fn keycloak_with_relative_path_auth_looks_legacy() {
    assert_eq!(
        observed(KC26_AUTHPREFIX),
        vec![
            pair("product_detected", "keycloak"),
            pair("debt_marker", "legacy_auth_path")
        ]
    );
}

#[test]
fn authentik_is_detected() {
    assert_eq!(
        observed(AUTHENTIK),
        vec![pair("product_detected", "authentik")]
    );
}

#[test]
fn zitadel_is_detected() {
    assert_eq!(observed(ZITADEL), vec![pair("product_detected", "zitadel")]);
}

#[test]
fn ferriskey_is_excluded() {
    assert!(observed(FERRISKEY).is_empty());
}

#[test]
fn negatives_detect_nothing() {
    assert!(observed(NGINX).is_empty());
    assert!(observed(HTTPD).is_empty());
}

#[test]
fn missing_fixture_for_a_path_is_no_response() {
    let paths = probe_paths(&iam_profile());
    let item = probed_host(NGINX, &paths);
    assert_eq!(item.responses.len(), paths.len());
    let answered = item
        .responses
        .iter()
        .filter(|probed| probed.response.is_some())
        .count();
    assert_eq!(answered, 1);
}
