use super::*;
use crate::shared_channels::{
    CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION, CHANNEL_SUBSCRIPTION_STORE_VERSION,
    ChannelAutoUpdateState, ChannelPublisherIdentity, ChannelReleaseTarget, ChannelSkillPin,
    ChannelSkillProvenance, ChannelSubscribedSkill, ChannelSubscription,
    ChannelSubscriptionRemoteState, ChannelSubscriptionStore, ChannelUpdateChange,
    ChannelUpdateItem, ChannelUpdateItemState, ChannelUpdateSnapshot, ChannelUpdateStatus,
    SharedChannelErrorCode, ensure_generic_skill_mutation_allowed,
};
use serde_json::Value;

struct DataDirGuard {
    previous: Option<std::ffi::OsString>,
    _temp: tempfile::TempDir,
}

impl Drop for DataDirGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
            None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
        }
    }
}

fn sandbox_data_dir() -> DataDirGuard {
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    DataDirGuard {
        previous,
        _temp: temp,
    }
}

fn skill(id: &str, digest: char) -> ChannelSubscribedSkill {
    ChannelSubscribedSkill {
        id: id.into(),
        content_root: format!("skills/{id}"),
        release_content_hash: format!("sha256:{}", digest.to_string().repeat(64)),
        release_content_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
        baseline_hash: format!("sha256:{}", digest.to_string().repeat(64)),
        baseline_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: 42,
            repository_url: "https://github.com/acme/channel.git".into(),
            git_ref: "a".repeat(40),
            source_folder: format!("skills/{id}"),
        },
    }
}

fn release_target() -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: 1,
        tag_name: "channel-v000001".into(),
        commit_sha: "a".repeat(40),
    }
}

/// A subscription tracking two Skills, where `writer` is pinned and named by
/// the stored update review — the state that makes pruning non-trivial.
fn subscription() -> ChannelSubscription {
    ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        repository_url_aliases: Vec::new(),
        target: release_target(),
        skills: vec![skill("reader", 'd'), skill("writer", 'b')],
        known_skill_ids: vec!["reader".into(), "writer".into()],
        pins: vec![ChannelSkillPin {
            skill_id: "writer".into(),
            target: release_target(),
        }],
        last_update: Some(ChannelUpdateSnapshot {
            target: ChannelReleaseTarget {
                revision: 2,
                tag_name: "channel-v000002".into(),
                commit_sha: "c".repeat(40),
            },
            title: "Second release".into(),
            notes: "Upgrade writer".into(),
            publisher: ChannelPublisherIdentity {
                id: 9,
                login: "alice".into(),
            },
            published_at: "2026-08-06T00:00:00Z".into(),
            checked_at: "2026-08-06T01:00:00Z".into(),
            status: ChannelUpdateStatus::Blocked,
            acknowledgement_required: true,
            items: vec![ChannelUpdateItem {
                id: "writer".into(),
                change: ChannelUpdateChange::Updated,
                state: ChannelUpdateItemState::Failed,
                selected: true,
                from_content_hash: Some(format!("sha256:{}", "b".repeat(64))),
                to_content_hash: Some(format!("sha256:{}", "c".repeat(64))),
                block_reason: None,
                suggested_local_name: None,
                error: Some("apply failed".into()),
                pinned_target: Some(release_target()),
                error_code: Some(SharedChannelErrorCode::SubscriptionUpdateFailed),
            }],
            check_error: None,
            check_error_code: None,
        }),
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn save(subscription: ChannelSubscription) {
    DiskChannelSubscriptionRegistry
        .save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription],
        })
        .unwrap();
}

/// A global maintenance path that deletes a channel-owned Skill must release
/// the gate for that name. Without this the subscription keeps claiming it,
/// `generic_installed_skill_is_mutable` keeps answering "immutable", and the
/// user can neither reinstall nor delete a name with nothing behind it.
#[test]
fn bulk_removal_releases_the_gate_and_keeps_the_subscription() {
    let _guard = crate::lock_test_env();
    let _data_dir = sandbox_data_dir();
    save(subscription());

    assert!(ensure_generic_skill_mutation_allowed("writer").is_err());

    assert_eq!(prune_removed_skills(&["WRITER".to_string()]).unwrap(), 1);

    // The removed Skill is free again; the one still installed stays owned.
    ensure_generic_skill_mutation_allowed("writer").unwrap();
    assert!(ensure_generic_skill_mutation_allowed("reader").is_err());

    let store = DiskChannelSubscriptionRegistry.load_mutable().unwrap();
    let subscription = &store.subscriptions[0];
    assert_eq!(
        subscription
            .skills
            .iter()
            .map(|skill| skill.id.as_str())
            .collect::<Vec<_>>(),
        vec!["reader"],
        "only the removed Skill leaves the selection"
    );
    assert!(
        subscription.pins.is_empty(),
        "a pin on a Skill that is gone would fail store validation"
    );
    assert!(
        subscription.last_update.is_none(),
        "the stored review describes Skills that are no longer installed"
    );
    assert_eq!(
        subscription.known_skill_ids,
        vec!["reader".to_string(), "writer".to_string()],
        "the channel still offers both, so both stay reinstallable"
    );
    assert_ne!(subscription.updated_at, "2026-08-05T00:00:00Z");
}

/// Removing every tracked Skill leaves the subscription itself in place: the
/// channel binding survives a hub reset so the user can reinstall from it.
#[test]
fn removing_every_tracked_skill_keeps_the_channel_binding() {
    let _guard = crate::lock_test_env();
    let _data_dir = sandbox_data_dir();
    save(subscription());

    assert_eq!(
        prune_removed_skills(&["reader".to_string(), "writer".to_string()]).unwrap(),
        2
    );

    let store = DiskChannelSubscriptionRegistry.load_mutable().unwrap();
    assert_eq!(store.subscriptions.len(), 1);
    assert!(store.subscriptions[0].skills.is_empty());
    assert_eq!(store.subscriptions[0].repository_id, 42);
}

/// Names that no subscription tracks must not rewrite the store at all.
#[test]
fn removing_unmanaged_skills_leaves_the_store_untouched() {
    let _guard = crate::lock_test_env();
    let _data_dir = sandbox_data_dir();
    save(subscription());
    let before = std::fs::read(DiskChannelSubscriptionRegistry::path()).unwrap();

    assert_eq!(prune_removed_skills(&["unrelated".to_string()]).unwrap(), 0);

    assert_eq!(
        std::fs::read(DiskChannelSubscriptionRegistry::path()).unwrap(),
        before
    );
}

/// A store on an unsupported schema cannot be rewritten. Pruning must report
/// that instead of returning success, so the caller aborts the reset rather
/// than deleting Skills the store will keep claiming forever.
#[test]
fn pruning_an_unsupported_schema_fails_closed() {
    let _guard = crate::lock_test_env();
    let _data_dir = sandbox_data_dir();
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["schema_version"] = Value::from(99);
    let payload = serde_json::to_vec(&value).unwrap();
    skillstar_core::infra::fs_ops::atomic_write(&DiskChannelSubscriptionRegistry::path(), &payload)
        .unwrap();

    let error = prune_removed_skills(&["writer".to_string()]).unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionSchemaUnsupported
    );
    assert_eq!(
        std::fs::read(DiskChannelSubscriptionRegistry::path()).unwrap(),
        payload,
        "a store that cannot be understood must not be rewritten"
    );
}

/// The preserved-file list is what a config reset consults; it has to name the
/// real store paths or the reset silently deletes them.
#[test]
fn provenance_paths_name_the_registry_and_subscription_stores() {
    let _guard = crate::lock_test_env();
    let _data_dir = sandbox_data_dir();

    assert_eq!(
        provenance_paths(),
        vec![
            DiskSharedChannelRegistry::path(),
            DiskChannelSubscriptionRegistry::path(),
        ]
    );
}
