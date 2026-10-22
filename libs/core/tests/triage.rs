mod support;

use chrono::{DateTime, Utc};
use lodestone_core::{
    ActiveProfile, Candidate, Clock, CreateProfileCommand, LeadRepository, LodestoneUseCase,
    NormalizedDomain, ProfileId, ProfileName, Provenance, RunRepository, RunStatus, SignalKind,
    SignalRepository, SourceError, SourceName, SuppressionTarget, ValidatedProfile,
    definition::AgentId,
    infrastructure::{
        lead::postgres::PgLeadRepository, postgres::with_tx, run::postgres::PgRunRepository,
        signal::postgres::PgSignalRepository,
    },
    triage::CandidateSource,
};
use serde_json::{Value, from_str, json};
use sqlx::PgPool;
use support::{at, domain, seen_lead};

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        at(100)
    }
}

struct FakeSource {
    candidates: Vec<Candidate>,
}

impl CandidateSource for FakeSource {
    fn name(&self) -> SourceName {
        SourceName::Boamp
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

fn candidate(host: &str, title: &str) -> Candidate {
    Candidate {
        domain: domain(host),
        source: SourceName::Boamp,
        kind: SignalKind::parse("public_tender").unwrap(),
        title: title.to_owned(),
        text: "Avis de marche public".to_owned(),
        provenance: Provenance::parse(&format!("https://marches.example/avis/{host}")).unwrap(),
        observed_at: at(50),
        details: json!({"idweb": host}),
    }
}

async fn activate(pool: &PgPool, name: &str, definition: Value) -> ProfileId {
    let usecase = LodestoneUseCase::new(pool.clone());
    let id = usecase
        .create_profile(CreateProfileCommand {
            name: ProfileName::parse(name).unwrap(),
            definition: ValidatedProfile::parse(definition).unwrap(),
            created_by: None,
        })
        .await
        .unwrap()
        .profile
        .id;
    usecase.set_profile_active(id, true).await.unwrap();
    id
}

async fn iam_profile(pool: &PgPool) -> ProfileId {
    activate(pool, "iam", serde_json::from_str(IAM).unwrap()).await
}

async fn run_triage(pool: &PgPool, source: &FakeSource) -> lodestone_core::triage::TriageReport {
    LodestoneUseCase::new(pool.clone())
        .run_triage(source, &FixedClock)
        .await
        .unwrap()
}

async fn signals(pool: &PgPool, profile: ProfileId, host: &str) -> Vec<lodestone_core::Signal> {
    with_tx(pool, async |shared| {
        let Some(lead) = PgLeadRepository::new(&shared)
            .find_by_domain(profile, &NormalizedDomain::from_host(host).unwrap())
            .await?
        else {
            return Ok(Vec::new());
        };
        PgSignalRepository::new(&shared)
            .list_for_lead(lead.id)
            .await
    })
    .await
    .unwrap()
}

async fn runs(pool: &PgPool) -> Vec<lodestone_core::Run> {
    with_tx(pool, async |shared| {
        PgRunRepository::new(&shared).list_recent(10).await
    })
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn ambiguous_candidates_become_leads_without_any_llm_call(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("mairie.fr", "Mise en place d'un SSO avec MFA"),
            candidate("meubles.fr", "Fourniture de mobilier de bureau"),
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("region.fr", "Projet IAM pour la region"),
        ],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 4);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.accepted_ambiguous, 1);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.rejected_by_exclusions, 1);
    assert_eq!(report.accepted(), 2);

    let accepted = signals(&pool, profile, "mairie.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].source, SourceName::Boamp);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(accepted[0].raw["details"]["idweb"], "mairie.fr");
    assert_eq!(accepted[0].strength.get(), 0.7);
    assert!(signals(&pool, profile, "meubles.fr").await.is_empty());

    assert!(signals(&pool, profile, "acme.com").await.is_empty());

    let ambiguous = signals(&pool, profile, "region.fr").await;
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].raw["route"], "keywords_ambiguous");
    assert_eq!(ambiguous[0].raw["matched"], json!(["IAM"]));
    assert_eq!(ambiguous[0].raw.get("excluded"), None);
    assert_eq!(ambiguous[0].raw["details"]["idweb"], "region.fr");
    assert_eq!(ambiguous[0].strength.get(), 0.5);
    assert!(ambiguous[0].strength.get() < accepted[0].strength.get());

    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
    assert_eq!(recorded[0].volume, 4);
    assert_eq!(recorded[0].signals, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_matching_exclusion_rejects_the_candidate_whatever_its_keywords(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![
            candidate("acme.com", "Badge d'acces et SSO"),
            candidate("mairie.fr", "SSO, MFA et controle d'acces physique"),
            candidate("ville.fr", "Annuaire TELEPHONIQUE avec SSO et MFA"),
            candidate("badges.fr", "Fourniture de badges"),
        ],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 4);
    assert_eq!(report.rejected_by_exclusions, 3);
    assert_eq!(report.rejected_by_rules, 1);
    assert_eq!(report.accepted_by_rules, 0);
    assert_eq!(report.accepted_ambiguous, 0);
    assert_eq!(report.accepted(), 0);
    for host in ["acme.com", "mairie.fr", "ville.fr", "badges.fr"] {
        assert!(signals(&pool, profile, host).await.is_empty());
        assert!(!has_lead(&pool, profile, host).await);
    }
    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].status, RunStatus::Succeeded);
    assert_eq!(recorded[0].volume, 4);
    assert_eq!(recorded[0].signals, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn keywords_match_without_accents_case_or_hyphens(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![boamp_agent(
            "boamp-fr",
            true,
            &["identite", "single sign-on"],
            1,
        )],
    )
    .await;
    let source = FakeSource {
        candidates: vec![
            candidate("ville.fr", "Gestion de l'IDENTITÉ"),
            candidate("region.fr", "Projet de Single Sign On"),
            candidate("meubles.fr", "Fourniture de mobilier"),
        ],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.accepted_by_rules, 2);
    assert_eq!(report.rejected_by_rules, 1);
    let accepted = signals(&pool, profile, "ville.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].raw["matched"], json!(["identite"]));
    assert_eq!(signals(&pool, profile, "region.fr").await.len(), 1);
    assert!(signals(&pool, profile, "meubles.fr").await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_second_run_does_not_duplicate_signals(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let source = FakeSource {
        candidates: vec![candidate("acme.com", "Projet IAM pour acme")],
    };

    run_triage(&pool, &source).await;
    run_triage(&pool, &source).await;

    assert_eq!(signals(&pool, profile, "acme.com").await.len(), 1);
    assert_eq!(runs(&pool).await.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn suppressed_domains_record_nothing(pool: PgPool) {
    let profile = iam_profile(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .add_suppression(SuppressionTarget::Domain(domain("bad.fr")), None, None)
        .await
        .unwrap();
    let source = FakeSource {
        candidates: vec![
            candidate("bad.fr", "Projet SSO pour bad"),
            candidate("good.fr", "Projet SSO pour good"),
        ],
    };

    run_triage(&pool, &source).await;

    assert!(signals(&pool, profile, "bad.fr").await.is_empty());
    assert_eq!(signals(&pool, profile, "good.fr").await.len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_route_the_same_candidate_with_their_own_rules(pool: PgPool) {
    let strict = iam_profile(&pool).await;
    let mut lenient_definition: Value = serde_json::from_str(IAM).unwrap();
    let boamp = lenient_definition["agents"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|agent| agent["id"] == "marches-publics")
        .unwrap();
    boamp["keywords"] = json!(["SSO"]);
    boamp["accept_min_keywords"] = json!(1);
    let lenient = activate(&pool, "lenient", lenient_definition).await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.accepted_ambiguous, 1);
    let ambiguous = signals(&pool, strict, "ville.fr").await;
    assert_eq!(ambiguous.len(), 1);
    assert_eq!(ambiguous[0].raw["route"], "keywords_ambiguous");
    let accepted = signals(&pool, lenient, "ville.fr").await;
    assert_eq!(accepted.len(), 1);
    assert_eq!(accepted[0].raw["route"], "keywords");
    assert_eq!(runs(&pool).await.len(), 2);
}

fn search_agent(id: &str, enabled: bool, providers: &[&str], keywords: &[&str], min: u8) -> Value {
    json!({
        "type": "search",
        "id": id,
        "name": id,
        "enabled": enabled,
        "providers": providers,
        "keywords": keywords,
        "accept_min_keywords": min,
    })
}

fn boamp_agent(id: &str, enabled: bool, keywords: &[&str], min: u8) -> Value {
    search_agent(id, enabled, &["boamp"], keywords, min)
}

async fn boamp_profile(pool: &PgPool, agents: Vec<Value>) -> ProfileId {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["agents"] = Value::Array(agents);
    activate(pool, "boamp-agents", definition).await
}

fn agent_ids(runs: &[lodestone_core::Run]) -> Vec<String> {
    let mut ids: Vec<String> = runs
        .iter()
        .filter_map(|run| run.agent.as_ref().map(|agent| agent.as_str().to_owned()))
        .collect();
    ids.sort();
    ids
}

#[sqlx::test(migrations = "../../migrations")]
async fn agents_of_the_same_kind_route_the_same_candidate_differently(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![
            boamp_agent("boamp-fr", true, &["SSO"], 1),
            boamp_agent("boamp-en", true, &["single sign-on"], 1),
        ],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 1);
    assert_eq!(report.accepted_by_rules, 1);
    assert_eq!(report.rejected_by_rules, 1);
    let recorded = signals(&pool, profile, "ville.fr").await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].agent, Some(AgentId::new("boamp-fr")));
    let runs = runs(&pool).await;
    assert_eq!(agent_ids(&runs), ["boamp-en", "boamp-fr"]);
    for run in &runs {
        let accepted = run.agent == Some(AgentId::new("boamp-fr"));
        assert_eq!(run.signals, u64::from(accepted));
        assert_eq!(run.volume, 1);
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_disabled_agent_is_neither_run_nor_recorded(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![
            boamp_agent("boamp-fr", false, &["SSO"], 1),
            boamp_agent("boamp-en", true, &["single sign-on"], 1),
        ],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    run_triage(&pool, &source).await;

    assert!(signals(&pool, profile, "ville.fr").await.is_empty());
    assert_eq!(agent_ids(&runs(&pool).await), ["boamp-en"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_profile_without_an_enabled_agent_of_the_kind_gets_no_run(pool: PgPool) {
    boamp_profile(&pool, vec![boamp_agent("boamp-fr", false, &["SSO"], 1)]).await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 0);
    assert!(runs(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_agent_over_several_providers_runs_once_per_provider_source(pool: PgPool) {
    let profile = boamp_profile(
        &pool,
        vec![search_agent(
            "marches",
            true,
            &["boamp", "ted", "decp"],
            &["SSO"],
            1,
        )],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.accepted_by_rules, 1);
    let recorded = signals(&pool, profile, "ville.fr").await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].source, SourceName::Boamp);
    assert_eq!(recorded[0].agent, Some(AgentId::new("marches")));
    assert_eq!(agent_ids(&runs(&pool).await), ["marches"]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_agent_not_attached_to_the_provider_is_not_run(pool: PgPool) {
    boamp_profile(
        &pool,
        vec![search_agent(
            "jobs",
            true,
            &["hacker_news_hiring", "france_travail"],
            &["SSO"],
            1,
        )],
    )
    .await;
    let source = FakeSource {
        candidates: vec![candidate("ville.fr", "Projet SSO pour la ville")],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.candidates, 0);
    assert!(runs(&pool).await.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn filters_drop_candidates_per_agent_after_collection(pool: PgPool) {
    let mut freelance = search_agent("freelance", true, &["boamp"], &["IAM"], 1);
    freelance["filters"] = json!({"contracts": ["freelance"], "remote": "remote_or_hybrid"});
    let open = search_agent("open", true, &["boamp"], &["IAM"], 1);
    let profile = boamp_profile(&pool, vec![freelance, open]).await;
    let mut offer = candidate("remote.fr", "Mission freelance IAM, full remote");
    offer.kind = SignalKind::parse("job_offer").unwrap();
    let mut onsite = candidate("onsite.fr", "Poste IAM en CDI, sur site");
    onsite.kind = SignalKind::parse("job_offer").unwrap();
    let source = FakeSource {
        candidates: vec![offer, onsite],
    };

    let report = run_triage(&pool, &source).await;

    assert_eq!(report.rejected_by_filters, 1);
    assert_eq!(report.accepted_by_rules, 3);
    let remote = signals(&pool, profile, "remote.fr").await;
    let mut agents: Vec<_> = remote
        .iter()
        .filter_map(|signal| signal.agent.as_ref().map(|agent| agent.as_str().to_owned()))
        .collect();
    agents.sort();
    assert_eq!(agents, ["freelance"]);
    let onsite = signals(&pool, profile, "onsite.fr").await;
    assert_eq!(onsite.len(), 1);
    assert_eq!(onsite[0].agent, Some(AgentId::new("open")));
    for run in runs(&pool).await {
        assert_eq!(run.volume, 2);
    }
}

struct CompanyJobsSource {
    candidates: Vec<Candidate>,
}

impl CandidateSource for CompanyJobsSource {
    fn name(&self) -> SourceName {
        SourceName::Ats
    }

    async fn collect(
        &self,
        _profiles: &[ActiveProfile],
        _now: DateTime<Utc>,
    ) -> Result<Vec<Candidate>, SourceError> {
        Ok(self.candidates.clone())
    }
}

async fn has_lead(pool: &PgPool, profile: ProfileId, host: &str) -> bool {
    with_tx(pool, async |shared| {
        PgLeadRepository::new(&shared)
            .find_by_domain(profile, &domain(host))
            .await
    })
    .await
    .unwrap()
    .is_some()
}

#[sqlx::test(migrations = "../../migrations")]
async fn company_job_offers_only_complete_leads_of_their_own_profile(pool: PgPool) {
    let first = iam_profile(&pool).await;
    let second = activate(&pool, "iam-b", from_str(IAM).unwrap()).await;
    seen_lead(&pool, first, "acme.com").await;
    let mut offer = candidate("acme.com", "Ingénieur IAM Keycloak");
    offer.source = SourceName::Ats;
    offer.kind = SignalKind::parse("job_offer").unwrap();
    let mut unrelated = candidate("acme.com", "Comptable");
    unrelated.source = SourceName::Ats;
    unrelated.kind = SignalKind::parse("job_offer").unwrap();
    let mut elsewhere = candidate("beta.io", "Architecte IAM Keycloak");
    elsewhere.source = SourceName::Ats;
    elsewhere.kind = SignalKind::parse("job_offer").unwrap();
    let source = CompanyJobsSource {
        candidates: vec![offer, unrelated, elsewhere],
    };

    let report = LodestoneUseCase::new(pool.clone())
        .run_triage(&source, &FixedClock)
        .await
        .unwrap();

    assert_eq!(report.accepted_by_rules, 4);
    assert_eq!(report.rejected_by_rules, 2);
    assert_eq!(report.skipped_no_lead, 3);
    let found = signals(&pool, first, "acme.com").await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source, SourceName::Ats);
    assert_eq!(found[0].kind.as_str(), "job_offer");
    assert_eq!(found[0].agent, None);
    assert_eq!(found[0].raw["route"], "keywords");
    assert!(!has_lead(&pool, first, "beta.io").await);
    assert!(!has_lead(&pool, second, "acme.com").await);
    assert!(!has_lead(&pool, second, "beta.io").await);
    let recorded = runs(&pool).await;
    assert_eq!(recorded.len(), 2);
    assert!(recorded.iter().all(|run| run.agent.is_none()));
    assert!(recorded.iter().all(|run| run.source == SourceName::Ats));
    let mut signals: Vec<u64> = recorded.iter().map(|run| run.signals).collect();
    signals.sort_unstable();
    assert_eq!(signals, [0, 1]);
}
