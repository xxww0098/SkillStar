use super::channel_update_tests::{
    UpdateChannels, channel, fixtures, manifest, release_skill, service, service_with_channels,
    target_v2,
};
use super::*;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FailingChannels(SharedChannelErrorCode);

#[async_trait::async_trait]
impl SharedChannelRegistry for FailingChannels {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        Err(SharedChannelError::new(
            self.0,
            "shared channel registry unavailable",
        ))
    }

    fn save(&self, _store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        unreachable!("a failed registry load must never be followed by a save")
    }
}

#[tokio::test]
async fn definitive_access_loss_freezes_downloads_without_deleting_installed_skills() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::RepositoryNotFound);
    let before = subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .clone();

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);
    let subscription = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(
        subscription.remote_state.status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
    assert_eq!(subscription.skills, before);
    assert!(installer.applied.lock().unwrap().is_empty());
    assert!(installer.uninstalled.lock().unwrap().is_empty());
}

#[tokio::test]
async fn network_errors_mark_the_subscription_offline_without_deleting_local_content() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::Network);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Network);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Offline
    );
}

#[tokio::test]
async fn temporary_permission_errors_are_recoverable_without_claiming_revocation() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::PermissionDenied);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::RecoverableFailure
    );
}

#[tokio::test]
async fn missing_channel_descriptor_freezes_the_subscription_and_invalidates_the_old_snapshot() {
    let (gateway, subscriptions, installer) = fixtures();
    let channels = UpdateChannels(Arc::new(Mutex::new(SharedChannelStore {
        schema_version: SHARED_CHANNEL_STORE_VERSION,
        channels: vec![channel()],
    })));
    let app = service_with_channels(gateway, channels.clone(), subscriptions.clone(), installer);
    app.check_update(42).await.unwrap();
    channels.0.lock().unwrap().channels.clear();

    let snapshot = app.check_update(42).await.unwrap();

    assert_eq!(
        snapshot.check_error_code,
        Some(SharedChannelErrorCode::RepositoryNotFound)
    );
    let stored = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(
        stored.remote_state.status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
    assert!(stored.last_update.as_ref().unwrap().check_error.is_some());
}

#[tokio::test]
async fn inactive_channel_descriptor_is_persisted_during_review_and_apply() {
    let (gateway, subscriptions, installer) = fixtures();
    let mut pending = channel();
    pending.status = SharedChannelStatus::AwaitingAppInstallation;
    let channels = UpdateChannels(Arc::new(Mutex::new(SharedChannelStore {
        schema_version: SHARED_CHANNEL_STORE_VERSION,
        channels: vec![pending],
    })));
    let app = service_with_channels(gateway, channels, subscriptions.clone(), installer.clone());

    let review_error = app.review(42).await.unwrap_err();

    assert_eq!(review_error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::RecoverableFailure
    );

    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::default();
    let apply_error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(apply_error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::RecoverableFailure
    );
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unreadable_or_future_channel_registry_freezes_an_existing_subscription() {
    for (registry_error, expected_error, expected_status) in [
        (
            SharedChannelErrorCode::SubscriptionSchemaUnsupported,
            SharedChannelErrorCode::SubscriptionSchemaUnsupported,
            ChannelSubscriptionRemoteStatus::IntegrityError,
        ),
        (
            SharedChannelErrorCode::Storage,
            SharedChannelErrorCode::Protocol,
            ChannelSubscriptionRemoteStatus::RecoverableFailure,
        ),
    ] {
        let (gateway, subscriptions, installer) = fixtures();
        service(gateway.clone(), subscriptions.clone(), installer.clone())
            .check_update(42)
            .await
            .unwrap();
        let app = ChannelSubscriptionFacade::new(
            gateway,
            FailingChannels(registry_error),
            subscriptions.clone(),
            installer,
        );

        let snapshot = app.check_update(42).await.unwrap();

        assert_eq!(snapshot.check_error_code, Some(expected_error));
        assert_eq!(
            subscriptions.store.lock().unwrap().subscriptions[0]
                .remote_state
                .status,
            expected_status
        );
    }
}

/// The mutation gate is injected at runtime, so nothing in the type system
/// proves the generic write paths in `skillstar-skills` are actually wired to
/// it — a missed `install_global_policy()` in a new entry point would silently
/// let users uninstall or overwrite channel-owned Skills through the ordinary
/// paths, desynchronising the subscription from disk. These assertions are that
/// proof: with `ChannelAwarePolicy` installed, each generic path must refuse.
#[test]
fn channel_owned_skills_are_refused_by_the_generic_skills_write_paths() {
    // Take the env lock *before* swapping the process-wide policy: the policy
    // is global, so installing it outside the lock would change the gate under
    // whichever test currently holds the lock and owns the data dir.
    let _guard = crate::lock_test_env();
    let _policy = skillstar_skills::skill_mutation::replace_skill_mutation_policy_for_test(
        Arc::new(crate::policy::ChannelAwarePolicy),
    );
    let temp = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
    }

    let result = (|| -> anyhow::Result<()> {
        let (_, subscriptions, _) = fixtures();
        DiskChannelSubscriptionRegistry.save(&subscriptions.store.lock().unwrap().clone())?;
        let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("writer");
        std::fs::create_dir_all(&hub_skill)?;
        std::fs::write(hub_skill.join("SKILL.md"), "# channel-owned\n")?;

        // Assert on the gate's own wording, not merely on `is_err`: every one
        // of these paths can fail for unrelated reasons, and a test that only
        // checked "it failed" would keep passing after the gate was unwired.
        let refused_by_gate = |message: String| {
            assert!(
                message.contains("managed by shared channel repository"),
                "expected the shared-channel gate to refuse this, got: {message}"
            );
        };

        let uninstall = skillstar_skills::skill_install::uninstall_skill("writer");
        refused_by_gate(uninstall.expect_err("uninstall_skill must refuse a channel-owned Skill"));
        assert!(
            hub_skill.join("SKILL.md").is_file(),
            "a refused uninstall must not have touched the content"
        );

        let create = skillstar_skills::local_skill::create("writer", Some("# mine\n"));
        refused_by_gate(
            create
                .expect_err("local_skill::create must refuse to shadow a channel-owned Skill")
                .to_string(),
        );

        let update = skillstar_skills::skill_update::update_skill("writer");
        refused_by_gate(
            update
                .expect_err("update_skill must refuse a channel-owned Skill")
                .to_string(),
        );

        // The same paths stay open for a Skill no channel manages.
        let unmanaged = skillstar_skills::local_skill::create("unmanaged", Some("# mine\n"));
        assert!(
            unmanaged.is_ok(),
            "the gate must only block channel-owned Skills, got {unmanaged:?}"
        );
        Ok(())
    })();

    unsafe {
        match previous_data {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
    result.unwrap();
}

#[test]
fn legacy_local_migration_skips_a_channel_owned_hub_directory() {
    // Scoped, and inside the env lock: `install_global_policy` would leak the
    // policy into every later test in this binary, and installing it before the
    // lock would apply it to whichever test is currently running.
    let _guard = crate::lock_test_env();
    let _policy = skillstar_skills::skill_mutation::replace_skill_mutation_policy_for_test(
        Arc::new(crate::policy::ChannelAwarePolicy),
    );
    let temp = tempfile::tempdir().unwrap();
    let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
    let previous_hub = std::env::var_os("SKILLSTAR_HUB_DIR");
    unsafe {
        std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        std::env::set_var("SKILLSTAR_HUB_DIR", temp.path().join("hub"));
    }

    let result = (|| -> anyhow::Result<()> {
        let (_, subscriptions, _) = fixtures();
        DiskChannelSubscriptionRegistry.save(&subscriptions.store.lock().unwrap().clone())?;
        let hub_skill = skillstar_core::infra::paths::hub_skills_dir().join("writer");
        std::fs::create_dir_all(&hub_skill)?;
        std::fs::write(hub_skill.join("SKILL.md"), "# channel-owned\n")?;

        let migrated = skillstar_skills::local_skill::migrate_existing()?;

        assert_eq!(migrated, 0);
        assert!(hub_skill.is_dir());
        assert!(
            !skillstar_core::infra::paths::local_skills_dir()
                .join("writer")
                .exists()
        );
        Ok(())
    })();

    unsafe {
        match previous_data {
            Some(value) => std::env::set_var("SKILLSTAR_DATA_DIR", value),
            None => std::env::remove_var("SKILLSTAR_DATA_DIR"),
        }
        match previous_hub {
            Some(value) => std::env::set_var("SKILLSTAR_HUB_DIR", value),
            None => std::env::remove_var("SKILLSTAR_HUB_DIR"),
        }
    }
    result.unwrap();
}

#[tokio::test]
async fn protocol_errors_are_persisted_as_recoverable_failures() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::Protocol);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Protocol);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::RecoverableFailure
    );
}

#[tokio::test]
async fn transferred_repository_identity_is_an_integrity_error() {
    let (gateway, subscriptions, installer) = fixtures();
    gateway.repository.lock().unwrap().owner_id = 99;
    gateway.repository.lock().unwrap().owner_login = "other-org".into();
    gateway.repository.lock().unwrap().html_url = "https://github.com/other-org/channel".into();
    gateway.repository.lock().unwrap().clone_url =
        "https://github.com/other-org/channel.git".into();
    let before = subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .clone();
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::OrganizationUnavailable);
    let stored = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(
        stored.remote_state.status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
    assert_eq!(stored.skills, before);
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn manifest_tamper_matrix_freezes_the_subscription_before_content_changes() {
    for case in ["schema", "repository", "duplicate", "traversal"] {
        let (gateway, subscriptions, installer) = fixtures();
        let mut tampered = manifest(
            2,
            'd',
            vec![release_skill("reader", 'e'), release_skill("writer", 'f')],
        );
        match case {
            "schema" => tampered.schema_version += 1,
            "repository" => tampered.repository_id = 99,
            "duplicate" => tampered.skills.push(tampered.skills[0].clone()),
            "traversal" => tampered.skills[0].content_root = "../reader".into(),
            _ => unreachable!(),
        }
        *gateway.manifests.lock().unwrap() = vec![tampered];
        let before = subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .clone();
        let app = service(gateway, subscriptions.clone(), installer.clone());

        let error = app.check_update(42).await.unwrap_err();

        assert_eq!(error.code, SharedChannelErrorCode::Integrity, "{case}");
        let stored = &subscriptions.store.lock().unwrap().subscriptions[0];
        assert_eq!(
            stored.remote_state.status,
            ChannelSubscriptionRemoteStatus::IntegrityError,
            "{case}"
        );
        assert_eq!(stored.skills, before, "{case}");
        assert!(installer.applied.lock().unwrap().is_empty(), "{case}");
    }
}

#[tokio::test]
async fn content_integrity_failure_during_apply_rolls_back_and_freezes_the_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("writer".into());
    installer
        .failure_codes
        .lock()
        .unwrap()
        .insert("writer".into(), SharedChannelErrorCode::Integrity);
    let before = subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .clone();
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    let stored = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(
        stored.remote_state.status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
    assert_eq!(stored.skills, before);
    assert_eq!(*installer.rollbacks.lock().unwrap(), vec!["reader"]);
}

#[tokio::test]
async fn a_visible_repository_without_read_permission_is_definitive_access_loss() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.has_read_access.lock().unwrap() = false;

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::AppRepositoryAccessRequired
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
}

#[tokio::test]
async fn review_persists_definitive_access_loss_for_an_existing_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::RepositoryNotFound);

    let error = app.review(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
}

#[tokio::test]
async fn apply_persists_access_loss_that_happens_after_the_last_check() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::RepositoryNotFound);

    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn git_access_loss_during_apply_rolls_back_and_freezes_the_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("writer".into());
    installer.failure_codes.lock().unwrap().insert(
        "writer".into(),
        SharedChannelErrorCode::AppRepositoryAccessRequired,
    );
    let before = subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .clone();
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::AppRepositoryAccessRequired
    );
    let subscription = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(
        subscription.remote_state.status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
    assert_eq!(subscription.skills, before);
    assert_eq!(*installer.rollbacks.lock().unwrap(), vec!["reader"]);
}

#[tokio::test]
async fn rollback_history_persists_access_loss_that_happens_after_the_last_check() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::RepositoryNotFound);

    let error = app
        .list_skill_rollback_targets(42, "writer")
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
}

#[tokio::test]
async fn a_successful_access_probe_unfreezes_the_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("access removed");
    let app = service(gateway, subscriptions.clone(), installer);

    app.check_update(42).await.unwrap();

    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Active
    );
}

#[tokio::test]
async fn invalid_release_keeps_the_subscription_frozen_as_an_integrity_error() {
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut store = subscriptions.store.lock().unwrap();
        store.subscriptions[0].remote_state =
            ChannelSubscriptionRemoteState::revoked("access removed");
        store.subscriptions[0].last_update = None;
    }
    *gateway.manifests.lock().unwrap() = vec![manifest(
        1,
        'f',
        vec![release_skill("reader", 'a'), release_skill("writer", 'c')],
    )];
    let app = service(gateway, subscriptions.clone(), installer);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
}

#[tokio::test]
async fn a_deleted_release_keeps_the_subscription_frozen_as_an_integrity_error() {
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut store = subscriptions.store.lock().unwrap();
        store.subscriptions[0].remote_state =
            ChannelSubscriptionRemoteState::revoked("access removed");
        store.subscriptions[0].last_update = None;
    }
    gateway.manifests.lock().unwrap().clear();
    let app = service(gateway, subscriptions.clone(), installer);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::ReleaseNotFound);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
}

#[tokio::test]
async fn successful_full_probe_recovers_offline_and_integrity_states() {
    for status in [
        ChannelSubscriptionRemoteStatus::Offline,
        ChannelSubscriptionRemoteStatus::RecoverableFailure,
        ChannelSubscriptionRemoteStatus::IntegrityError,
    ] {
        let (gateway, subscriptions, installer) = fixtures();
        subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
            ChannelSubscriptionRemoteState {
                status,
                checked_at: Some("2026-08-05T00:00:00Z".into()),
                message: Some("frozen".into()),
            };
        let app = service(gateway, subscriptions.clone(), installer);

        app.check_update(42).await.unwrap();

        assert_eq!(
            subscriptions.store.lock().unwrap().subscriptions[0]
                .remote_state
                .status,
            ChannelSubscriptionRemoteStatus::Active
        );
    }
}

#[tokio::test]
async fn repository_rename_refreshes_registry_routing_by_stable_id() {
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut repository = gateway.repository.lock().unwrap();
        repository.name = "renamed-channel".into();
        repository.html_url = "https://github.com/acme/renamed-channel".into();
        repository.clone_url = "https://github.com/acme/renamed-channel.git".into();
    }
    let channels = UpdateChannels(Arc::new(Mutex::new(SharedChannelStore {
        schema_version: SHARED_CHANNEL_STORE_VERSION,
        channels: vec![channel()],
    })));
    let app = service_with_channels(gateway, channels.clone(), subscriptions.clone(), installer);

    app.check_update(42).await.unwrap();

    let stored = channels.0.lock().unwrap();
    assert_eq!(stored.channels[0].repository_id, 42);
    assert_eq!(stored.channels[0].name, "renamed-channel");
    assert_eq!(
        stored.channels[0].clone_url,
        "https://github.com/acme/renamed-channel.git"
    );
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.provenance.repository_url
                == "https://github.com/acme/renamed-channel.git")
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0].repository_url_aliases,
        vec!["https://github.com/acme/channel.git"]
    );
}

#[tokio::test]
async fn direct_apply_refreshes_renamed_repository_routing_by_stable_id() {
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut repository = gateway.repository.lock().unwrap();
        repository.name = "renamed-channel".into();
        repository.html_url = "https://github.com/acme/renamed-channel".into();
        repository.clone_url = "https://github.com/acme/renamed-channel.git".into();
    }
    let channels = UpdateChannels(Arc::new(Mutex::new(SharedChannelStore {
        schema_version: SHARED_CHANNEL_STORE_VERSION,
        channels: vec![channel()],
    })));
    let app = service_with_channels(gateway, channels.clone(), subscriptions.clone(), installer);

    app.apply_update(ApplyChannelUpdateRequest {
        repository_id: 42,
        target: target_v2(),
        resolutions: Vec::new(),
    })
    .await
    .unwrap();

    let stored = channels.0.lock().unwrap();
    assert_eq!(stored.channels[0].repository_id, 42);
    assert_eq!(stored.channels[0].name, "renamed-channel");
    assert_eq!(
        stored.channels[0].clone_url,
        "https://github.com/acme/renamed-channel.git"
    );
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.provenance.repository_url
                == "https://github.com/acme/renamed-channel.git")
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0].repository_url_aliases,
        vec!["https://github.com/acme/channel.git"]
    );
}

#[tokio::test]
async fn duplicate_release_revisions_freeze_the_subscription_as_an_integrity_failure() {
    let (gateway, subscriptions, installer) = fixtures();
    gateway
        .manifests
        .lock()
        .unwrap()
        .push(super::channel_update_tests::manifest_v2());
    let app = service(gateway, subscriptions.clone(), installer);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
}

#[tokio::test]
async fn frozen_subscriptions_reject_remote_mutations_before_downloading_content() {
    let (gateway, subscriptions, installer) = fixtures();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("access removed");
    let app = service(gateway, subscriptions, installer.clone());

    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionAccessRevoked
    );
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn every_non_active_remote_state_rejects_remote_mutations() {
    for status in [
        ChannelSubscriptionRemoteStatus::Offline,
        ChannelSubscriptionRemoteStatus::RecoverableFailure,
        ChannelSubscriptionRemoteStatus::IntegrityError,
    ] {
        let (gateway, subscriptions, installer) = fixtures();
        subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
            ChannelSubscriptionRemoteState {
                status,
                checked_at: Some("2026-08-05T00:00:00Z".into()),
                message: Some("frozen".into()),
            };
        let app = service(gateway, subscriptions, installer.clone());

        let error = app
            .apply_update(ApplyChannelUpdateRequest {
                repository_id: 42,
                target: target_v2(),
                resolutions: Vec::new(),
            })
            .await
            .unwrap_err();

        assert!(matches!(
            error.code,
            SharedChannelErrorCode::Network
                | SharedChannelErrorCode::Protocol
                | SharedChannelErrorCode::Integrity
        ));
        assert!(installer.applied.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn frozen_skills_can_be_uninstalled_without_remote_access() {
    let (gateway, subscriptions, installer) = fixtures();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("access removed");
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::Network);
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let result = app.uninstall_revoked_skill(42, "writer").await.unwrap();

    assert_eq!(*installer.uninstalled.lock().unwrap(), vec!["writer"]);
    assert!(
        result
            .subscription
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );
    assert_eq!(
        result.subscription.remote_state.status,
        ChannelSubscriptionRemoteStatus::Revoked
    );
}

#[tokio::test]
async fn frozen_skills_can_be_converted_to_local_without_remote_access() {
    let (gateway, subscriptions, installer) = fixtures();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("access removed");
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::Network);
    let app = service(gateway, subscriptions, installer.clone());

    let result = app
        .convert_revoked_skill_to_local(ConvertRemovedChannelSkillRequest {
            repository_id: 42,
            skill_id: "writer".into(),
            local_name: "writer.local".into(),
        })
        .await
        .unwrap();

    assert_eq!(result.local_name.as_deref(), Some("writer.local"));
    assert_eq!(
        *installer.converted.lock().unwrap(),
        vec![("writer".into(), "writer.local".into())]
    );
    assert!(
        result
            .subscription
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );
}

#[tokio::test]
async fn resolving_one_frozen_skill_preserves_other_pending_removal_tombstones() {
    let (gateway, subscriptions, installer) = fixtures();
    let mut removed_writer = release_skill("writer", 'f');
    removed_writer.status = ChannelSkillReleaseStatus::Removed;
    *gateway.manifests.lock().unwrap() = vec![manifest(
        2,
        'd',
        vec![release_skill("reader", 'e'), removed_writer],
    )];
    let app = service(gateway, subscriptions.clone(), installer);
    app.check_update(42).await.unwrap();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("access removed");

    app.uninstall_revoked_skill(42, "reader").await.unwrap();

    {
        let stored = subscriptions.store.lock().unwrap();
        let writer = stored.subscriptions[0]
            .last_update
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == "writer")
            .unwrap();
        assert_eq!(writer.state, ChannelUpdateItemState::RemovedFromChannel);
    }

    app.uninstall_revoked_skill(42, "writer").await.unwrap();

    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .last_update
            .is_none()
    );
}
#[test]
fn github_rate_limits_are_retryable_without_claiming_the_network_is_offline() {
    for (status, body) in [
        (429, r#"{"message":"too many requests"}"#),
        (403, r#"{"message":"API rate limit exceeded"}"#),
    ] {
        let error = super::github::ensure_status(status, body, &[200]).unwrap_err();
        assert_eq!(error.code, SharedChannelErrorCode::Protocol);
        assert!(error.message.contains("rate-limited"));
    }
}
