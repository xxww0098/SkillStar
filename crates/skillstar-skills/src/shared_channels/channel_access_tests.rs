use super::channel_update_tests::{fixtures, manifest, release_skill, service, target_v2};
use super::*;

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
async fn temporary_repository_errors_keep_the_last_known_access_state() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::Network);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Network);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Active
    );
}

#[tokio::test]
async fn generic_permission_errors_do_not_prove_that_repository_access_was_revoked() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    *gateway.repository_error.lock().unwrap() = Some(SharedChannelErrorCode::PermissionDenied);

    let error = app.check_update(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .remote_state
            .status,
        ChannelSubscriptionRemoteStatus::Active
    );
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
async fn restored_access_is_persisted_even_when_the_release_is_invalid_after_the_probe() {
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
        ChannelSubscriptionRemoteStatus::Active
    );
}

#[tokio::test]
async fn restored_access_is_persisted_even_when_no_release_is_available() {
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
        ChannelSubscriptionRemoteStatus::Active
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
