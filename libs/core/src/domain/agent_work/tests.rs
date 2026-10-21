use std::{slice::from_ref, time::Duration};

use chrono::{Duration as Delta, TimeZone, Utc};
use serde_json::{Value, from_str, json};
use tokio::time::timeout;
use uuid::Uuid;

use super::*;
use crate::domain::profile::{Profile, ProfileName, ProfileRevision, ValidatedProfile};

const IAM: &str = include_str!("../../../../../docs/examples/profiles/iam-ferriskey.json");

fn current(revision: u32, active: bool, edit: impl FnOnce(&mut Value)) -> ActiveProfile {
    let mut document: Value = from_str(IAM).unwrap();
    edit(&mut document);
    let at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
    ActiveProfile {
        profile: Profile {
            id: ProfileId(Uuid::from_u128(7)),
            name: ProfileName::parse("IAM").unwrap(),
            active,
            created_at: at,
            updated_at: at,
        },
        revision: ProfileRevision {
            profile_id: ProfileId(Uuid::from_u128(7)),
            number: RevisionNumber::new(revision).unwrap(),
            definition: ValidatedProfile::parse(document).unwrap(),
            created_by: None,
            created_at: at,
        },
    }
}

fn qualification(id: &str) -> WorkTarget {
    WorkTarget::new(WorkKind::Qualification, &AgentId::new(id))
}

fn snapshot_of(profile: &ActiveProfile) -> WorkSnapshot {
    let mut snapshot = WorkSnapshot::default();
    snapshot.apply(profile.profile.id, ProfileAgents::of(profile));
    snapshot
}

#[test]
fn a_present_enabled_agent_is_live_and_the_aggregator_always_is() {
    let profile = current(1, true, |_| {});
    let snapshot = snapshot_of(&profile);
    assert_eq!(
        snapshot.check(profile.profile.id, &qualification("marche")),
        None
    );
    assert_eq!(
        snapshot.check(profile.profile.id, &WorkTarget::aggregator()),
        None
    );
    assert_eq!(
        snapshot.check(
            profile.profile.id,
            &WorkTarget::new(WorkKind::Sourcing, &AgentId::new("dns"))
        ),
        None
    );
}

#[test]
fn a_removed_agent_and_a_disabled_agent_have_their_own_reason() {
    let profile = current(2, true, |document| {
        let agents = document["qualification_agents"].as_array_mut().unwrap();
        agents.retain(|agent| agent["id"] != "marche");
        for agent in agents.iter_mut() {
            if agent["id"] == "opportunite" {
                agent["enabled"] = json!(false);
            }
        }
        document["enrichment"]["custom"]["fingerprints"] = json!([]);
    });
    let snapshot = snapshot_of(&profile);
    let id = profile.profile.id;
    assert_eq!(
        snapshot.check(id, &qualification("marche")),
        Some(CancelReason::AgentRemoved)
    );
    assert_eq!(
        snapshot.check(id, &qualification("opportunite")),
        Some(CancelReason::AgentDisabled)
    );
    assert_eq!(
        snapshot.check(id, &WorkTarget::enrichment(EnrichmentStep::HttpProbe)),
        Some(CancelReason::AgentDisabled)
    );
    assert_eq!(snapshot.check(id, &qualification("pertinence")), None);
}

#[test]
fn a_deactivated_profile_stops_every_agent_and_a_deleted_one_never_comes_back() {
    let profile = current(1, true, |_| {});
    let id = profile.profile.id;
    let mut snapshot = snapshot_of(&profile);
    let mut inactive = current(1, false, |_| {});
    inactive.profile.updated_at += Delta::seconds(1);
    assert!(snapshot.apply(id, ProfileAgents::of(&inactive)));
    assert_eq!(
        snapshot.check(id, &WorkTarget::aggregator()),
        Some(CancelReason::ProfileDeactivated)
    );
    assert!(snapshot.delete(id));
    assert!(!snapshot.apply(id, ProfileAgents::of(&current(3, true, |_| {}))));
    assert_eq!(
        snapshot.check(id, &qualification("marche")),
        Some(CancelReason::ProfileDeleted)
    );
}

#[test]
fn an_older_state_never_replaces_a_newer_one() {
    let profile = current(2, true, |document| {
        document["qualification_agents"]
            .as_array_mut()
            .unwrap()
            .retain(|agent| agent["id"] != "marche");
    });
    let id = profile.profile.id;
    let mut snapshot = snapshot_of(&profile);
    assert!(!snapshot.apply(id, ProfileAgents::of(&current(1, true, |_| {}))));
    assert_eq!(
        snapshot.check(id, &qualification("marche")),
        Some(CancelReason::AgentRemoved)
    );
}

#[test]
fn a_refresh_marks_missing_profiles_deleted_until_they_are_seen_again() {
    let profile = current(1, true, |_| {});
    let id = profile.profile.id;
    let mut snapshot = snapshot_of(&profile);
    assert!(snapshot.refresh(&[]));
    assert_eq!(
        snapshot.check(id, &qualification("marche")),
        Some(CancelReason::ProfileDeleted)
    );
    assert!(snapshot.refresh(from_ref(&profile)));
    assert_eq!(snapshot.check(id, &qualification("marche")), None);
}

#[test]
fn an_unknown_profile_is_not_stopped() {
    let snapshot = WorkSnapshot::default();
    assert_eq!(
        snapshot.check(ProfileId(Uuid::from_u128(9)), &qualification("x")),
        None
    );
}

#[tokio::test]
async fn a_token_resolves_when_its_agent_is_removed() {
    let profile = current(1, true, |_| {});
    let id = profile.profile.id;
    let (sender, receiver) = watch::channel(Arc::new(snapshot_of(&profile)));
    let work = AgentWork::new(
        id,
        profile.revision.number,
        qualification("marche"),
        receiver,
    );
    assert!(work.is_live());
    assert!(
        timeout(Duration::from_millis(20), work.cancelled())
            .await
            .is_err()
    );
    let removed = current(2, true, |document| {
        document["qualification_agents"]
            .as_array_mut()
            .unwrap()
            .retain(|agent| agent["id"] != "marche");
    });
    sender.send_modify(|snapshot| {
        Arc::make_mut(snapshot).apply(id, ProfileAgents::of(&removed));
    });
    assert_eq!(
        timeout(Duration::from_secs(1), work.cancelled())
            .await
            .unwrap(),
        CancelReason::AgentRemoved
    );
    assert!(!work.is_live());
    assert_eq!(first_stop(&[work]), Some(CancelReason::AgentRemoved));
}

#[test]
fn enabled_lists_the_live_agents_of_a_kind() {
    let profile = current(1, true, |_| {});
    let agents = ProfileAgents::of(&profile);
    assert_eq!(
        agents
            .enabled(WorkKind::Scoring)
            .iter()
            .map(|agent| agent.as_str())
            .collect::<Vec<_>>(),
        vec!["score-global"]
    );
}

#[test]
fn reasons_round_trip_through_text() {
    for reason in [
        CancelReason::AgentRemoved,
        CancelReason::AgentDisabled,
        CancelReason::ProfileDeactivated,
        CancelReason::ProfileDeleted,
    ] {
        assert_eq!(CancelReason::parse(reason.as_str()), Some(reason));
    }
}

#[test]
fn removing_or_disabling_agents_only_is_detected_and_any_other_edit_is_not() {
    let previous = current(1, true, |_| {});
    let removed = current(2, true, |document| {
        document["qualification_agents"]
            .as_array_mut()
            .unwrap()
            .retain(|agent| agent["id"] != "marche");
        document["agents"]
            .as_array_mut()
            .unwrap()
            .retain(|agent| agent["id"] != "dns");
        document["enrichment"]["custom"]["fingerprints"] = json!([]);
    });
    let edited = current(2, true, |document| {
        document["qualification_agents"]
            .as_array_mut()
            .unwrap()
            .retain(|agent| agent["id"] != "marche");
        document["identity"]["offer"] = json!("Autre offre");
    });
    let before = previous.revision.definition.definition();
    assert!(only_removes_agents(
        before,
        removed.revision.definition.definition()
    ));
    assert!(!only_removes_agents(
        before,
        edited.revision.definition.definition()
    ));
    assert!(!only_removes_agents(before, before));
    let switched_off = current(2, true, |document| {
        document["inspection"]["enabled"] = json!(false);
    });
    assert!(only_removes_agents(
        before,
        switched_off.revision.definition.definition()
    ));
    let kept = kept_agents(&ProfileAgents::of(&removed));
    assert!(!kept.qualification.contains(&"marche".to_owned()));
    assert!(kept.qualification.contains(&"pertinence".to_owned()));
    assert_eq!(kept.scoring, vec!["score-global".to_owned()]);
}
