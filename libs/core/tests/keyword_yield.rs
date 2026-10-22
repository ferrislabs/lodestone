mod support;

use chrono::{Duration, Utc};
use common::CoreError;
use lodestone_core::{
    ActiveProfile, AgentYield, CreateProfileCommand, KeywordYield, LeadId, LodestoneUseCase,
    ProfileId, ProfileName, QualityReport, RevisionNumber, SuggestionAction, SuggestionEdit,
    UserId, ValidatedProfile,
    definition::{Agent, ProfileDefinition, SearchAgent},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use support::seen_lead;
use uuid::Uuid;

const IAM: &str = include_str!("../../../docs/examples/profiles/iam-ferriskey.json");

fn search_agent(id: &str, keywords: &[&str], exclusions: &[&str]) -> Value {
    json!({
        "type": "search",
        "id": id,
        "name": format!("Agent {id}"),
        "enabled": true,
        "providers": ["boamp"],
        "keywords": keywords,
        "exclusions": exclusions,
        "accept_min_keywords": 1,
    })
}

async fn profile_with(pool: &PgPool, name: &str, agents: Vec<Value>) -> ProfileId {
    let mut definition: Value = serde_json::from_str(IAM).unwrap();
    definition["agents"] = Value::Array(agents);
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

async fn user(pool: &PgPool) -> UserId {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, sub, username, display_name) VALUES ($1, $1::text, 'alice', 'Alice')")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    UserId(id)
}

async fn lead(pool: &PgPool, profile: ProfileId, host: &str, verdict: Option<&str>) -> LeadId {
    let lead = seen_lead(pool, profile, host).await;
    sqlx::query("UPDATE leads SET verdict = $2 WHERE id = $1")
        .bind(lead.id.0)
        .bind(verdict)
        .execute(pool)
        .await
        .unwrap();
    lead.id
}

async fn signal(
    pool: &PgPool,
    lead: LeadId,
    agent: &str,
    matched: &[&str],
    title: &str,
    days_ago: i32,
) {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint, agent_id)
         VALUES ($1, $2, 'job_offer', $3, 'boamp', $4, $5, 0.8, now() - make_interval(days => $6), $1::text, $7)",
    )
    .bind(id)
    .bind(lead.0)
    .bind(title)
    .bind(json!({"title": title, "matched": matched, "route": "accept"}))
    .bind(format!("https://jobs.example/{id}"))
    .bind(days_ago)
    .bind(agent)
    .execute(pool)
    .await
    .unwrap();
}

async fn decide(pool: &PgPool, lead: LeadId, profile: ProfileId, kind: &str) {
    sqlx::query(
        "INSERT INTO decision_labels (id, lead_id, profile_id, profile_revision, kind, decided_at, snapshot)
         VALUES ($1, $2, $3, 1, $4, now(), $5)",
    )
    .bind(Uuid::now_v7())
    .bind(lead.0)
    .bind(profile.0)
    .bind(kind)
    .bind(json!({
        "confidence": null,
        "opinion": null,
        "evidence": null,
        "verdict": null,
        "verdict_reason": null,
        "agents": {},
    }))
    .execute(pool)
    .await
    .unwrap();
}

async fn found(
    pool: &PgPool,
    profile: ProfileId,
    host: &str,
    agent: &str,
    matched: &[&str],
    title: &str,
) -> LeadId {
    let id = lead(pool, profile, host, None).await;
    signal(pool, id, agent, matched, title, 1).await;
    id
}

async fn report(pool: &PgPool, profile: ProfileId) -> QualityReport {
    LodestoneUseCase::new(pool.clone())
        .quality_report(None, None, Some(profile))
        .await
        .unwrap()
}

fn agent<'a>(report: &'a QualityReport, id: &str) -> &'a AgentYield {
    report
        .sourcing
        .agents
        .iter()
        .find(|row| row.agent_id == id)
        .unwrap()
}

fn keyword<'a>(agent: &'a AgentYield, word: &str) -> &'a KeywordYield {
    agent
        .keywords
        .iter()
        .find(|row| row.keyword == word)
        .unwrap()
}

fn suggested(report: &QualityReport, action: SuggestionAction) -> Vec<&str> {
    report
        .sourcing
        .suggestions
        .iter()
        .filter(|suggestion| suggestion.action == action)
        .map(|suggestion| suggestion.word.as_str())
        .collect()
}

fn edit(agent: &str, action: SuggestionAction, word: &str) -> SuggestionEdit {
    SuggestionEdit {
        agent_id: agent.to_owned(),
        action,
        word: word.to_owned(),
    }
}

fn revision(number: u32) -> RevisionNumber {
    RevisionNumber::new(number).unwrap()
}

async fn apply(
    usecase: &LodestoneUseCase,
    profile: ProfileId,
    actor: UserId,
    agent: &str,
    action: SuggestionAction,
    word: &str,
) -> Result<ActiveProfile, CoreError> {
    usecase
        .apply_keyword_suggestion(profile, &edit(agent, action, word), revision(1), actor)
        .await
}

fn search_of<'a>(definition: &'a ProfileDefinition, id: &str) -> &'a SearchAgent {
    definition
        .agents
        .iter()
        .filter_map(Agent::as_search)
        .find(|agent| agent.id.as_str() == id)
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_table_counts_leads_decisions_and_precision_per_agent_and_keyword(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "yield",
        vec![
            search_agent("jobs", &["sso", "mfa"], &[]),
            search_agent("tenders", &["oidc"], &[]),
        ],
    )
    .await;
    let a = lead(&pool, profile, "a.fr", Some("fiable")).await;
    signal(&pool, a, "jobs", &["sso", "mfa"], "Ingénieur SSO MFA", 1).await;
    decide(&pool, a, profile, "lead_validated").await;
    let b = lead(&pool, profile, "b.fr", Some("a_verifier")).await;
    signal(&pool, b, "jobs", &["sso"], "Développeur SSO", 1).await;
    decide(&pool, b, profile, "lead_rejected").await;
    let c = lead(&pool, profile, "c.fr", Some("ecarte")).await;
    signal(&pool, c, "jobs", &["mfa"], "Consultant MFA", 1).await;
    decide(&pool, c, profile, "lead_rejected").await;
    found(&pool, profile, "d.fr", "jobs", &["sso"], "Architecte SSO").await;
    let e = lead(&pool, profile, "e.fr", Some("fiable")).await;
    signal(&pool, e, "tenders", &["oidc"], "Marché OIDC", 1).await;
    decide(&pool, e, profile, "lead_validated").await;
    let f = lead(&pool, profile, "f.fr", None).await;
    signal(&pool, f, "tenders", &["oidc"], "Marché OIDC bis", 1).await;
    decide(&pool, f, profile, "lead_rejected").await;

    let report = report(&pool, profile).await;

    assert_eq!(report.sourcing.agents.len(), 2);
    let jobs = agent(&report, "jobs");
    assert_eq!(jobs.agent_name.as_deref(), Some("Agent jobs"));
    assert_eq!(jobs.leads, 4);
    assert_eq!(
        (
            jobs.verdicts.reliable,
            jobs.verdicts.to_verify,
            jobs.verdicts.discarded,
            jobs.verdicts.none
        ),
        (1, 1, 1, 1)
    );
    assert_eq!((jobs.validated, jobs.rejected), (1, 2));
    assert_eq!(jobs.precision, Some(1.0 / 3.0));
    let sso = keyword(jobs, "sso");
    assert_eq!((sso.leads, sso.exclusive_leads), (3, 2));
    assert_eq!((sso.validated, sso.rejected), (1, 1));
    assert_eq!(sso.precision, Some(0.5));
    let mfa = keyword(jobs, "mfa");
    assert_eq!((mfa.leads, mfa.exclusive_leads), (2, 1));
    assert_eq!((mfa.validated, mfa.rejected), (1, 1));
    assert_eq!(mfa.precision, Some(0.5));
    let names: Vec<&str> = jobs
        .keywords
        .iter()
        .map(|row| row.keyword.as_str())
        .collect();
    assert_eq!(names, ["sso", "mfa"]);

    let tenders = agent(&report, "tenders");
    assert_eq!(tenders.leads, 2);
    assert_eq!((tenders.validated, tenders.rejected), (1, 1));
    assert_eq!(tenders.precision, Some(0.5));
    let oidc = keyword(tenders, "oidc");
    assert_eq!((oidc.leads, oidc.exclusive_leads), (2, 2));
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_signals_observed_in_the_period_count(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "period",
        vec![search_agent("jobs", &["sso", "mfa"], &[])],
    )
    .await;
    let recent = lead(&pool, profile, "recent.fr", None).await;
    signal(&pool, recent, "jobs", &["sso"], "Récent", 2).await;
    let old = lead(&pool, profile, "old.fr", None).await;
    signal(&pool, old, "jobs", &["sso"], "Ancien", 60).await;

    let usecase = LodestoneUseCase::new(pool.clone());
    let narrow = usecase
        .quality_report(Some(Utc::now() - Duration::days(30)), None, Some(profile))
        .await
        .unwrap();
    assert_eq!(agent(&narrow, "jobs").leads, 1);
    let everything = usecase
        .quality_report(None, None, Some(profile))
        .await
        .unwrap();
    assert_eq!(agent(&everything, "jobs").leads, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn signals_without_an_agent_are_left_out(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "unattributed",
        vec![search_agent("jobs", &["sso", "mfa"], &[])],
    )
    .await;
    let id = lead(&pool, profile, "probe.fr", None).await;
    sqlx::query(
        "INSERT INTO signals (id, lead_id, kind, value, source, raw, provenance, strength, observed_at, fingerprint)
         VALUES ($1, $2, 'product_detected', 'keycloak', 'http_probe', '{\"matched\": \"keycloak\"}'::jsonb, 'https://probe.fr', 0.8, now(), $1::text)",
    )
    .bind(Uuid::now_v7())
    .bind(id.0)
    .execute(&pool)
    .await
    .unwrap();

    assert!(report(&pool, profile).await.sourcing.agents.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_keyword_rejected_above_the_threshold_is_suggested_for_removal(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "removal",
        vec![search_agent("jobs", &["sso", "mfa", "scim"], &[])],
    )
    .await;
    for index in 0..5 {
        let id = found(
            &pool,
            profile,
            &format!("sso{index}.fr"),
            "jobs",
            &["sso"],
            "Poste",
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }
    for index in 0..4 {
        let id = found(
            &pool,
            profile,
            &format!("mfa{index}.fr"),
            "jobs",
            &["mfa"],
            "Poste",
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }
    for index in 0..5 {
        let id = found(
            &pool,
            profile,
            &format!("scim{index}.fr"),
            "jobs",
            &["scim"],
            "Poste",
        )
        .await;
        decide(
            &pool,
            id,
            profile,
            if index == 0 {
                "lead_validated"
            } else {
                "lead_rejected"
            },
        )
        .await;
    }

    let report = report(&pool, profile).await;

    assert_eq!(suggested(&report, SuggestionAction::RemoveKeyword), ["sso"]);
    let suggestion = &report.sourcing.suggestions[0];
    assert_eq!(suggestion.profile_id, profile);
    assert_eq!(suggestion.agent_id, "jobs");
    assert_eq!((suggestion.decided, suggestion.rejected), (5, 5));
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_last_keyword_of_an_agent_is_not_suggested_for_removal(pool: PgPool) {
    let profile = profile_with(&pool, "last", vec![search_agent("jobs", &["sso"], &[])]).await;
    for index in 0..6 {
        let id = found(
            &pool,
            profile,
            &format!("sso{index}.fr"),
            "jobs",
            &["sso"],
            "Job",
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }

    assert!(report(&pool, profile).await.sourcing.suggestions.is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_word_found_only_in_rejected_titles_is_suggested_as_an_exclusion(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "exclusion",
        vec![search_agent("jobs", &["sso", "mfa"], &["freelance"])],
    )
    .await;
    for (index, title) in [
        "Stage ingénieur SSO",
        "Stage développeur",
        "Stage de fin d'études",
    ]
    .into_iter()
    .enumerate()
    {
        let id = found(
            &pool,
            profile,
            &format!("stage{index}.fr"),
            "jobs",
            &["sso"],
            title,
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }
    for (index, title) in ["Poste freelance", "Poste freelance", "Poste freelance"]
        .into_iter()
        .enumerate()
    {
        let id = found(
            &pool,
            profile,
            &format!("poste{index}.fr"),
            "jobs",
            &["mfa"],
            title,
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }
    let kept = found(&pool, profile, "kept.fr", "jobs", &["mfa"], "Poste senior").await;
    decide(&pool, kept, profile, "lead_validated").await;

    let report = report(&pool, profile).await;

    assert_eq!(
        suggested(&report, SuggestionAction::AddExclusion),
        ["stage"]
    );
    let suggestion = &report.sourcing.suggestions[0];
    assert_eq!((suggestion.decided, suggestion.rejected), (3, 3));
}

#[sqlx::test(migrations = "../../migrations")]
async fn applying_a_suggestion_writes_a_new_revision(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "apply",
        vec![
            search_agent("jobs", &["sso", "mfa"], &["freelance"]),
            search_agent("tenders", &["oidc", "saml"], &[]),
        ],
    )
    .await;
    let actor = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    let removed = usecase
        .apply_keyword_suggestion(
            profile,
            &edit("jobs", SuggestionAction::RemoveKeyword, "sso"),
            revision(1),
            actor,
        )
        .await
        .unwrap();
    assert_eq!(removed.revision.number, revision(2));
    assert_eq!(removed.revision.created_by, Some(actor));
    let definition = removed.revision.definition.definition();
    assert_eq!(search_of(definition, "jobs").keywords, ["mfa"]);
    assert_eq!(search_of(definition, "jobs").exclusions, ["freelance"]);
    assert_eq!(search_of(definition, "tenders").keywords, ["oidc", "saml"]);

    let excluded = usecase
        .apply_keyword_suggestion(
            profile,
            &edit("tenders", SuggestionAction::AddExclusion, "stage"),
            revision(2),
            actor,
        )
        .await
        .unwrap();
    assert_eq!(excluded.revision.number, revision(3));
    let definition = excluded.revision.definition.definition();
    assert_eq!(search_of(definition, "tenders").exclusions, ["stage"]);
    assert_eq!(search_of(definition, "jobs").keywords, ["mfa"]);

    let revisions = usecase.list_profile_revisions(profile).await.unwrap();
    assert_eq!(revisions.len(), 3);
    let first = revisions
        .iter()
        .find(|candidate| candidate.number == revision(1))
        .unwrap();
    assert_eq!(
        search_of(first.definition.definition(), "jobs").keywords,
        ["sso", "mfa"]
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_stale_revision_is_a_conflict_and_changes_nothing(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "stale",
        vec![search_agent("jobs", &["sso", "mfa"], &[])],
    )
    .await;
    let actor = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());
    usecase
        .apply_keyword_suggestion(
            profile,
            &edit("jobs", SuggestionAction::AddExclusion, "stage"),
            revision(1),
            actor,
        )
        .await
        .unwrap();

    let stale = usecase
        .apply_keyword_suggestion(
            profile,
            &edit("jobs", SuggestionAction::RemoveKeyword, "sso"),
            revision(1),
            actor,
        )
        .await;

    assert!(matches!(stale, Err(CoreError::Conflict(_))));
    assert_eq!(
        usecase.list_profile_revisions(profile).await.unwrap().len(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_suggestion_that_no_longer_applies_is_refused(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "refused",
        vec![
            search_agent("jobs", &["sso", "mfa"], &["stage"]),
            search_agent("solo", &["oidc"], &[]),
        ],
    )
    .await;
    let actor = user(&pool).await;
    let usecase = LodestoneUseCase::new(pool.clone());

    assert!(matches!(
        apply(
            &usecase,
            profile,
            actor,
            "jobs",
            SuggestionAction::RemoveKeyword,
            "scim"
        )
        .await,
        Err(CoreError::Conflict(_))
    ));
    assert!(matches!(
        apply(
            &usecase,
            profile,
            actor,
            "jobs",
            SuggestionAction::AddExclusion,
            "Stage"
        )
        .await,
        Err(CoreError::Conflict(_))
    ));
    assert!(matches!(
        apply(
            &usecase,
            profile,
            actor,
            "solo",
            SuggestionAction::RemoveKeyword,
            "oidc"
        )
        .await,
        Err(CoreError::Conflict(_))
    ));
    assert!(matches!(
        apply(
            &usecase,
            profile,
            actor,
            "ghost",
            SuggestionAction::RemoveKeyword,
            "sso"
        )
        .await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        usecase
            .apply_keyword_suggestion(
                ProfileId(Uuid::now_v7()),
                &edit("jobs", SuggestionAction::RemoveKeyword, "sso"),
                revision(1),
                actor,
            )
            .await,
        Err(CoreError::NotFound)
    ));
    assert_eq!(
        usecase.list_profile_revisions(profile).await.unwrap().len(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_applied_removal_leaves_the_suggestions(pool: PgPool) {
    let profile = profile_with(
        &pool,
        "settled",
        vec![search_agent("jobs", &["sso", "mfa"], &[])],
    )
    .await;
    for index in 0..5 {
        let id = found(
            &pool,
            profile,
            &format!("sso{index}.fr"),
            "jobs",
            &["sso"],
            "Poste",
        )
        .await;
        decide(&pool, id, profile, "lead_rejected").await;
    }
    assert_eq!(
        suggested(
            &report(&pool, profile).await,
            SuggestionAction::RemoveKeyword
        ),
        ["sso"]
    );

    LodestoneUseCase::new(pool.clone())
        .apply_keyword_suggestion(
            profile,
            &edit("jobs", SuggestionAction::RemoveKeyword, "sso"),
            revision(1),
            user(&pool).await,
        )
        .await
        .unwrap();

    assert!(
        suggested(
            &report(&pool, profile).await,
            SuggestionAction::RemoveKeyword
        )
        .is_empty()
    );
}
