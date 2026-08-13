use super::*;
use crate::shared_channels::ensure_generic_skill_mutation_allowed;
use crate::shared_channels::{
    ChannelAutoUpdateState, ChannelPublisherIdentity, ChannelSkillProvenance,
    ChannelSubscribedSkill, ChannelSubscription, ChannelSubscriptionRemoteState,
    ChannelSubscriptionRemoteStatus, ChannelUpdateChange, ChannelUpdateItem,
    ChannelUpdateItemState, ChannelUpdateSnapshot, ChannelUpdateStatus,
};

fn subscription() -> ChannelSubscription {
    ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        repository_url_aliases: Vec::new(),
        target: ChannelReleaseTarget {
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: "a".repeat(40),
        },
        skills: vec![ChannelSubscribedSkill {
            id: "writer".into(),
            content_root: "skills/writer".into(),
            release_content_hash: format!("sha256:{}", "b".repeat(64)),
            release_content_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
            baseline_hash: format!("sha256:{}", "b".repeat(64)),
            baseline_hash_version: skillstar_skills::content::SNAPSHOT_HASH_VERSION,
            provenance: ChannelSkillProvenance {
                repository_id: 42,
                repository_url: "https://github.com/acme/channel.git".into(),
                git_ref: "a".repeat(40),
                source_folder: "skills/writer".into(),
            },
        }],
        known_skill_ids: vec!["writer".into()],
        pins: Vec::new(),
        last_update: None,
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

#[test]
fn unknown_schema_is_listed_read_only_but_mutation_is_rejected() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["schema_version"] = Value::from(99);
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    let registry = DiskChannelSubscriptionRegistry;
    let views = registry.list_views().unwrap();
    assert_eq!(views.len(), 1);
    assert!(views[0].read_only);
    assert_eq!(views[0].selected_skill_ids, vec!["writer"]);
    assert_eq!(
        registry.load_mutable().unwrap_err().code,
        SharedChannelErrorCode::SubscriptionSchemaUnsupported
    );
    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn malformed_future_subscription_fails_closed_for_generic_skill_mutation() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["schema_version"] = Value::from(99);
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("skills");
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let error = ensure_generic_skill_mutation_allowed("writer").unwrap_err();

    assert!(error.to_string().contains("cannot safely project"));
    assert_eq!(
        DiskChannelSubscriptionRegistry
            .list_views()
            .unwrap_err()
            .code,
        SharedChannelErrorCode::SubscriptionSchemaUnsupported
    );
    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn empty_selection_still_owns_the_registered_channel_repository_route() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };

    let mut empty = subscription();
    empty.skills.clear();
    empty.known_skill_ids.clear();
    DiskChannelSubscriptionRegistry
        .save(&ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![empty],
        })
        .unwrap();
    DiskSharedChannelRegistry
        .save(&crate::shared_channels::SharedChannelStore {
            schema_version: crate::shared_channels::SHARED_CHANNEL_STORE_VERSION,
            channels: vec![crate::shared_channels::SharedChannelDescriptor {
                descriptor_version: crate::shared_channels::CHANNEL_DESCRIPTOR_VERSION,
                repository_id: 42,
                organization_id: 7,
                owner: "acme".into(),
                name: "channel".into(),
                html_url: "https://github.com/acme/channel".into(),
                clone_url: "https://github.com/acme/channel.git".into(),
                role: crate::shared_channels::SharedChannelRole::Subscriber,
                status: crate::shared_channels::SharedChannelStatus::Active,
                authorization: crate::shared_channels::SharedChannelAuthorization::default(),
                created_at: String::new(),
                updated_at: String::new(),
            }],
        })
        .unwrap();

    assert_eq!(
        managed_repository_for_url("http://www.github.com:80/acme/channel").unwrap(),
        Some(42)
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn unknown_descriptor_is_listed_read_only_but_mutation_is_rejected() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["subscriptions"][0]["descriptor_version"] = Value::from(99);
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let registry = DiskChannelSubscriptionRegistry;
    let views = registry.list_views().unwrap();
    assert_eq!(views.len(), 1);
    assert!(views[0].read_only);
    assert_eq!(views[0].descriptor_version, 99);
    assert_eq!(
        registry.load_mutable().unwrap_err().code,
        SharedChannelErrorCode::SubscriptionSchemaUnsupported
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn version_three_subscriptions_default_to_active_remote_access_and_upgrade_in_memory() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["subscriptions"][0]["descriptor_version"] = Value::from(3);
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("remote_state");
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let loaded = DiskChannelSubscriptionRegistry.load_mutable().unwrap();
    assert_eq!(
        loaded.subscriptions[0].descriptor_version,
        CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION
    );
    assert_eq!(
        loaded.subscriptions[0].remote_state.status,
        ChannelSubscriptionRemoteStatus::Active
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn current_schema_rejects_duplicate_repository_views() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let duplicate = ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription(), subscription()],
    };
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&duplicate).unwrap(),
    )
    .unwrap();

    assert_eq!(
        DiskChannelSubscriptionRegistry
            .list_views()
            .unwrap_err()
            .code,
        SharedChannelErrorCode::Storage
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn pin_target_must_match_the_installed_skill_provenance() {
    let mut subscription = subscription();
    subscription
        .pins
        .push(crate::shared_channels::ChannelSkillPin {
            skill_id: subscription.skills[0].id.clone(),
            target: ChannelReleaseTarget {
                revision: subscription.target.revision,
                tag_name: subscription.target.tag_name.clone(),
                commit_sha: "f".repeat(40),
            },
        });

    let error = validate_store(&ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription],
    })
    .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
}

#[test]
fn known_store_round_trips_after_restart() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let registry = DiskChannelSubscriptionRegistry;
    let store = ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    };
    registry.save(&store).unwrap();

    let restarted = DiskChannelSubscriptionRegistry;
    assert_eq!(restarted.load_mutable().unwrap(), store);
    assert_eq!(
        restarted.list_views().unwrap()[0].selected_skill_ids,
        vec!["writer"]
    );
    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn automatic_update_run_lease_is_exclusive_across_registry_handles() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };

    let first = DiskChannelSubscriptionRegistry;
    let second = DiskChannelSubscriptionRegistry;
    let lease = first
        .try_acquire_auto_update_run_lease(42)
        .unwrap()
        .expect("first registry should acquire the channel run lease");
    assert!(
        second
            .try_acquire_auto_update_run_lease(42)
            .unwrap()
            .is_none()
    );
    drop(lease);
    assert!(
        second
            .try_acquire_auto_update_run_lease(42)
            .unwrap()
            .is_some()
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn partially_upgraded_skill_provenance_round_trips_independently() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut partial = subscription();
    partial.skills[0].provenance.git_ref = "b".repeat(40);
    let store = ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![partial],
    };

    let registry = DiskChannelSubscriptionRegistry;
    registry.save(&store).unwrap();
    assert_eq!(registry.load_mutable().unwrap(), store);

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn previous_descriptor_is_migrated_without_becoming_read_only() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![subscription()],
    })
    .unwrap();
    value["subscriptions"][0]["descriptor_version"] = Value::from(1);
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("last_update");
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("pins");
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("auto_update");
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let registry = DiskChannelSubscriptionRegistry;
    let view = registry.list_views().unwrap().remove(0);
    assert!(!view.read_only);
    assert_eq!(
        view.descriptor_version,
        CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION
    );
    let migrated = registry.load_mutable().unwrap();
    assert_eq!(
        migrated.subscriptions[0].descriptor_version,
        CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION
    );
    assert!(migrated.subscriptions[0].last_update.is_none());
    assert!(migrated.subscriptions[0].pins.is_empty());
    assert_eq!(
        migrated.subscriptions[0].auto_update,
        ChannelAutoUpdateState::default()
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}

#[test]
fn previous_descriptor_backfills_structured_update_error_codes() {
    let _guard = crate::lock_test_env();
    let temp = tempfile::tempdir().unwrap();
    let previous = std::env::var_os("SKILLSTAR_DATA_DIR");
    unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", temp.path()) };
    let mut prior = subscription();
    prior.descriptor_version = 2;
    prior.last_update = Some(ChannelUpdateSnapshot {
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
            error: Some("legacy apply failure".into()),
            pinned_target: None,
            error_code: Some(SharedChannelErrorCode::SubscriptionUpdateFailed),
        }],
        check_error: Some("legacy check failure".into()),
        check_error_code: Some(SharedChannelErrorCode::Network),
    });
    let mut value = serde_json::to_value(ChannelSubscriptionStore {
        schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
        subscriptions: vec![prior],
    })
    .unwrap();
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("pins");
    value["subscriptions"][0]
        .as_object_mut()
        .unwrap()
        .remove("auto_update");
    value["subscriptions"][0]["last_update"]
        .as_object_mut()
        .unwrap()
        .remove("check_error_code");
    value["subscriptions"][0]["last_update"]["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("error_code");
    skillstar_core::infra::fs_ops::atomic_write(
        &DiskChannelSubscriptionRegistry::path(),
        &serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();

    let migrated = DiskChannelSubscriptionRegistry.load_mutable().unwrap();
    let subscription = &migrated.subscriptions[0];
    let snapshot = subscription.last_update.as_ref().unwrap();
    assert_eq!(
        snapshot.check_error_code,
        Some(SharedChannelErrorCode::Protocol)
    );
    assert_eq!(
        snapshot.items[0].error_code,
        Some(SharedChannelErrorCode::SubscriptionUpdateFailed)
    );
    assert_eq!(
        subscription.descriptor_version,
        CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("SKILLSTAR_DATA_DIR", value) },
        None => unsafe { std::env::remove_var("SKILLSTAR_DATA_DIR") },
    }
}
