use common::CoreError;
use lodestone_core::{
    CreateProfileCommand, LodestoneUseCase, ProfileId, ProfileName, ReviseProfileCommand,
    ValidatedProfile,
};
use sqlx::PgPool;
use uuid::Uuid;

const MINIMAL_PROFILE: &str = r#"{
    "schema_version": 1,
    "identity": {"offer": "Managed IAM", "target_customer": "Companies running Keycloak"},
    "sources": {
        "http_probe": {
            "fingerprints": [{
                "id": "keycloak-master-realm",
                "path": "/realms/master",
                "product": "keycloak",
                "matchers": [
                    {"kind": "status", "equals": 200},
                    {"kind": "body", "pattern": "\"realm\"\\s*:\\s*\"master\""}
                ],
                "version": {"from": "header", "name": "x-keycloak-version", "pattern": "^(\\d+\\.\\d+)"},
                "debt_markers": [{"id": "old", "rule": {"kind": "version_below", "version": "22.0"}}]
            }],
            "excluded_products": ["ferriskey"]
        }
    },
    "scoring": {"factors": [{"signal": "product_detected", "value": "keycloak", "weight": 10.0}]},
    "contact_ranking": {"preferred_local_parts": ["security", "contact"]},
    "llm": {"qualification_threshold": 50.0, "relevance_criteria": "Self-hosts an IAM product"}
}"#;

fn definition(threshold: f64) -> ValidatedProfile {
    let mut value: serde_json::Value = serde_json::from_str(MINIMAL_PROFILE).unwrap();
    value["llm"]["qualification_threshold"] = serde_json::json!(threshold);
    ValidatedProfile::parse(value).unwrap()
}

fn create(name: &str, threshold: f64) -> CreateProfileCommand {
    CreateProfileCommand {
        name: ProfileName::parse(name).unwrap(),
        definition: definition(threshold),
        created_by: None,
    }
}

fn revise(id: ProfileId, threshold: f64) -> ReviseProfileCommand {
    ReviseProfileCommand {
        profile_id: id,
        definition: definition(threshold),
        created_by: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_then_list(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let created = uc.create_profile(create("alpha", 50.0)).await.unwrap();

    let profiles = uc.list_profiles().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].id, created.profile.id);
    assert!(!profiles[0].active);

    let revisions = uc.list_profile_revisions(created.profile.id).await.unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].number.get(), 1);
    assert_eq!(revisions[0].definition, definition(50.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn revise_twice_and_activate(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let id = uc
        .create_profile(create("alpha", 50.0))
        .await
        .unwrap()
        .profile
        .id;
    uc.revise_profile(revise(id, 60.0)).await.unwrap();
    uc.revise_profile(revise(id, 70.0)).await.unwrap();

    let numbers: Vec<u32> = uc
        .list_profile_revisions(id)
        .await
        .unwrap()
        .iter()
        .map(|r| r.number.get())
        .collect();
    assert_eq!(numbers, vec![1, 2, 3]);

    uc.set_profile_active(id, true).await.unwrap();
    let active = uc.list_active_profiles().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].revision.number.get(), 3);
    assert_eq!(active[0].revision.definition, definition(70.0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn deactivate_keeps_revisions(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let id = uc
        .create_profile(create("alpha", 50.0))
        .await
        .unwrap()
        .profile
        .id;
    uc.set_profile_active(id, true).await.unwrap();
    assert_eq!(uc.list_active_profiles().await.unwrap().len(), 1);

    uc.set_profile_active(id, false).await.unwrap();
    assert!(uc.list_active_profiles().await.unwrap().is_empty());
    assert_eq!(uc.list_profile_revisions(id).await.unwrap().len(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn duplicate_name_conflicts(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    uc.create_profile(create("alpha", 50.0)).await.unwrap();
    let result = uc.create_profile(create("alpha", 60.0)).await;
    assert!(matches!(result, Err(CoreError::Conflict(_))));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_profile_is_not_found(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let id = ProfileId(Uuid::new_v4());
    assert!(matches!(
        uc.revise_profile(revise(id, 60.0)).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        uc.set_profile_active(id, true).await,
        Err(CoreError::NotFound)
    ));
    assert!(matches!(
        uc.list_profile_revisions(id).await,
        Err(CoreError::NotFound)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn revisions_are_immutable(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool.clone());
    uc.create_profile(create("alpha", 50.0)).await.unwrap();

    let result = sqlx::query("UPDATE profile_revisions SET created_at = now()")
        .execute(&pool)
        .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_revision_is_visible_without_caching(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let id = uc
        .create_profile(create("alpha", 50.0))
        .await
        .unwrap()
        .profile
        .id;
    uc.set_profile_active(id, true).await.unwrap();
    let first = uc.list_active_profiles().await.unwrap();
    assert_eq!(first[0].revision.number.get(), 1);

    uc.revise_profile(revise(id, 80.0)).await.unwrap();
    let second = uc.list_active_profiles().await.unwrap();
    assert_eq!(second[0].revision.number.get(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn several_active_profiles_each_latest(pool: PgPool) {
    let uc = LodestoneUseCase::new(pool);
    let a = uc
        .create_profile(create("alpha", 50.0))
        .await
        .unwrap()
        .profile
        .id;
    let b = uc
        .create_profile(create("beta", 55.0))
        .await
        .unwrap()
        .profile
        .id;
    uc.revise_profile(revise(a, 60.0)).await.unwrap();
    uc.revise_profile(revise(b, 65.0)).await.unwrap();
    uc.revise_profile(revise(b, 75.0)).await.unwrap();
    uc.set_profile_active(a, true).await.unwrap();
    uc.set_profile_active(b, true).await.unwrap();

    let active = uc.list_active_profiles().await.unwrap();
    assert_eq!(active.len(), 2);
    assert_eq!(active[0].profile.id, a);
    assert_eq!(active[0].revision.number.get(), 2);
    assert_eq!(active[0].revision.definition, definition(60.0));
    assert_eq!(active[1].profile.id, b);
    assert_eq!(active[1].revision.number.get(), 3);
    assert_eq!(active[1].revision.definition, definition(75.0));
}
