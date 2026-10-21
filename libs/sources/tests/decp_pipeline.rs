use std::{sync::Arc, time::Duration};

use chrono::{DateTime, TimeZone, Utc};
use lodestone_core::{
    Clock, CreateProfileCommand, FetchError, FetchRequest, FetchResponse, Fetcher, LeadRepository,
    LodestoneUseCase, NormalizedDomain, Partner, PartnerRepository, ProfileId, ProfileName,
    SourceName, ValidatedProfile,
    infrastructure::{
        lead::postgres::PgLeadRepository, partner::postgres::PgPartnerRepository, postgres::with_tx,
    },
};
use serde_json::json;
use sources::decp::{
    ANNUAIRE_URL, DATASET_API_URL, DecpSettings, DecpSource, ENTREPRISES_URL, partner_awards,
};
use sqlx::PgPool;
use url::Url;

const DATASET: &str = include_str!("fixtures/decp/dataset.json");
const APRIL: &str = include_str!("fixtures/decp/decp-2026-04.json");
const MARCH: &str = include_str!("fixtures/decp/decp-2026-03.json");
const ANNUAIRE_LORIENT: &str = include_str!("fixtures/decp/annuaire-21560121200016.json");
const ANNUAIRE_DEPARTMENT: &str = include_str!("fixtures/decp/annuaire-22350001800013.json");
const ANNUAIRE_AGGLO: &str = include_str!("fixtures/decp/annuaire-20004217400090.json");
const LIKSI: &str = include_str!("fixtures/decp/entreprises-83177947500027.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).unwrap()
    }
}

struct FixtureFetcher;

fn body(text: &str) -> Result<FetchResponse, FetchError> {
    Ok(FetchResponse {
        status: 200,
        headers: Vec::new(),
        body: text.as_bytes().to_vec(),
        final_url: Url::parse("https://example.test/").unwrap(),
        truncated: false,
    })
}

impl Fetcher for FixtureFetcher {
    async fn fetch(&self, request: &FetchRequest) -> Result<FetchResponse, FetchError> {
        let url = &request.url;
        let query = |name: &str| {
            url.query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default()
        };
        match url.host_str().unwrap() {
            "www.data.gouv.fr" => body(DATASET),
            "static.data.gouv.fr" if url.path().ends_with("decp-2026-04.json") => body(APRIL),
            "static.data.gouv.fr" => body(MARCH),
            "api-lannuaire.service-public.fr" => body(match query("where").as_str() {
                "siret=\"21560121200016\"" => ANNUAIRE_LORIENT,
                "siret=\"22350001800013\"" => ANNUAIRE_DEPARTMENT,
                "siret=\"20004217400090\"" => ANNUAIRE_AGGLO,
                _ => r#"{"total_count":0,"results":[]}"#,
            }),
            _ if query("q") == "83177947500027" => body(LIKSI),
            _ => body(r#"{"results":[]}"#),
        }
    }
}

fn settings() -> DecpSettings {
    DecpSettings {
        dataset_api_url: Url::parse(DATASET_API_URL).unwrap().into(),
        annuaire_url: Url::parse(ANNUAIRE_URL).unwrap(),
        entreprises_url: Url::parse(ENTREPRISES_URL).unwrap().into(),
        lookback_days: 31,
        notified_within_days: 31,
        max_body_bytes: 1024 * 1024,
        max_rate_limit_wait: Duration::from_secs(1),
    }
}

async fn activate(pool: &PgPool, name: &str, decp: bool) -> ProfileId {
    let mut sources = json!({
        "dns_cname": {"patterns": [{"provider": "auth0", "target_suffix": "auth0.com"}]}
    });
    if decp {
        sources["decp"] = json!({
            "keywords": ["SSO", "annuaire", "authentification", "MFA"],
            "exclusions": ["annuaire téléphonique"]
        });
    }
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: ValidatedProfile::parse(json!({
                "schema_version": 1,
                "identity": {"offer": "Managed IAM", "target_customer": "Companies"},
                "sources": sources,
                "scoring": {"factors": [{"signal": "saas_provider", "weight": 10.0}]},
                "contact_ranking": {"preferred_local_parts": ["security"]},
                "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Uses an IAM"}
            }))
            .unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn run_job(pool: &PgPool, source: &DecpSource<FixtureFetcher>) {
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase.run_triage(source, &FixedClock).await.unwrap();
    let contracts = source.take_awards();
    for active in usecase.list_active_profiles().await.unwrap() {
        let Some(rules) = active
            .revision
            .definition
            .definition()
            .enabled_search_agents(SourceName::Decp)
            .next()
        else {
            continue;
        };
        let awards = partner_awards(rules, &contracts);
        usecase
            .record_partner_awards(active.profile.id, &awards, FixedClock.now())
            .await
            .unwrap();
    }
}

async fn partners(pool: &PgPool, profile: ProfileId) -> Vec<Partner> {
    with_tx(pool, async |shared| {
        PgPartnerRepository::new(&shared)
            .list_for_profile(profile)
            .await
    })
    .await
    .unwrap()
}

async fn has_lead(pool: &PgPool, profile: ProfileId, host: &str) -> bool {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(host).unwrap())
            .await
    })
    .await
    .unwrap()
    .is_some()
}

#[sqlx::test(migrations = "../../migrations")]
async fn buyers_become_leads_and_holders_become_partners(pool: PgPool) {
    let profile = activate(&pool, "iam", true).await;
    let other = activate(&pool, "no-decp", false).await;
    let source = DecpSource::new(Arc::new(FixtureFetcher), settings());
    run_job(&pool, &source).await;

    assert!(has_lead(&pool, profile, "ille-et-vilaine.fr").await);
    assert!(has_lead(&pool, profile, "lorient-agglo.bzh").await);
    assert!(has_lead(&pool, profile, "lorient.bzh").await);
    assert!(!has_lead(&pool, profile, "megalis.bretagne.bzh").await);
    assert!(!has_lead(&pool, other, "ille-et-vilaine.fr").await);

    let found = partners(&pool, profile).await;
    let sirets: Vec<&str> = found.iter().map(|partner| partner.siret.as_str()).collect();
    assert_eq!(
        sirets,
        ["83177947500027", "34927581800061", "81378068100025"]
    );
    let liksi = &found[0];
    assert_eq!(liksi.name.as_deref(), Some("LIKSI RENNES"));
    assert_eq!(liksi.award_count, 3);
    assert_eq!(
        liksi.last_award_at.map(|date| date.to_string()).as_deref(),
        Some("2026-04-14")
    );
    assert!(partners(&pool, other).await.is_empty());
    assert!(!has_lead(&pool, profile, "liksi.fr").await);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_daily_run_changes_nothing(pool: PgPool) {
    let profile = activate(&pool, "iam", true).await;
    let source = DecpSource::new(Arc::new(FixtureFetcher), settings());
    run_job(&pool, &source).await;
    let first = partners(&pool, profile).await;
    run_job(&pool, &source).await;
    assert_eq!(partners(&pool, profile).await, first);
    let awards = sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM partner_awards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(awards, 5);
    let signals = sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM signals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(signals, 3);
}
