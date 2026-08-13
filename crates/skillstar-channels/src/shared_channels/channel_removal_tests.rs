use super::channel_update_tests::{
    fixtures, manifest, manifest_v1, release_skill, service, target_v2,
};
use super::*;

fn removed_manifest() -> ChannelReleaseManifest {
    let mut writer = release_skill("writer", 'f');
    writer.status = ChannelSkillReleaseStatus::Removed;
    manifest(2, 'd', vec![release_skill("reader", 'e'), writer])
}

fn reintroduced_manifest() -> ChannelReleaseManifest {
    manifest(
        3,
        '7',
        vec![release_skill("reader", 'e'), release_skill("writer", '8')],
    )
}

async fn prepare_removed_release(
    gateway: &super::channel_update_tests::UpdateGateway,
    app: &ChannelSubscriptionFacade<
        super::channel_update_tests::UpdateGateway,
        super::channel_update_tests::UpdateChannels,
        super::channel_update_tests::UpdateSubscriptions,
        super::channel_update_tests::UpdateInstaller,
    >,
) -> ChannelUpdateSnapshot {
    *gateway.manifests.lock().unwrap() = vec![manifest_v1(), removed_manifest()];
    app.check_update(42).await.unwrap()
}

#[tokio::test]
async fn removed_skill_keeps_content_and_deployments_while_other_skills_continue() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());

    let checked = prepare_removed_release(&gateway, &app).await;
    let removed = checked
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(removed.change, ChannelUpdateChange::Removed);
    assert_eq!(removed.state, ChannelUpdateItemState::RemovedFromChannel);
    assert_eq!(
        removed.suggested_local_name.as_deref(),
        Some("writer.local")
    );
    assert!(installer.uninstalled.lock().unwrap().is_empty());
    assert!(installer.converted.lock().unwrap().is_empty());

    let applied = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(applied.applied_skill_ids, vec!["reader"]);
    let store = subscriptions.store.lock().unwrap();
    assert_eq!(store.subscriptions[0].skills.len(), 2);
    assert_eq!(
        store.subscriptions[0]
            .skills
            .iter()
            .find(|skill| skill.id == "writer")
            .unwrap()
            .release_content_hash,
        manifest_v1().skills[1].content_hash
    );
}

#[tokio::test]
async fn uninstall_uses_existing_cleanup_and_stops_tracking_the_removed_skill() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    app.apply_update(ApplyChannelUpdateRequest {
        repository_id: 42,
        target: target_v2(),
        resolutions: Vec::new(),
    })
    .await
    .unwrap();
    subscriptions.store.lock().unwrap().subscriptions[0]
        .pins
        .push(ChannelSkillPin {
            skill_id: "writer".into(),
            target: super::subscription::release_target(&manifest_v1()),
        });

    let result = app.uninstall_removed_skill(42, "writer").await.unwrap();

    assert_eq!(*installer.uninstalled.lock().unwrap(), vec!["writer"]);
    assert_eq!(result.skill_id, "writer");
    assert_eq!(result.local_name, None);
    let store = subscriptions.store.lock().unwrap();
    assert!(
        store.subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );
    assert!(
        !store.subscriptions[0]
            .known_skill_ids
            .iter()
            .any(|id| id == "writer")
    );
    assert!(store.subscriptions[0].pins.is_empty());
    assert_eq!(result.snapshot.status, ChannelUpdateStatus::UpToDate);
    assert_eq!(store.subscriptions[0].target.revision, 2);
}

#[tokio::test]
async fn exact_release_verification_freezes_removal_before_local_content_changes() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    *installer.release_verification_error.lock().unwrap() = Some(SharedChannelErrorCode::Integrity);

    let error = app.uninstall_removed_skill(42, "writer").await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(installer.uninstalled.lock().unwrap().is_empty());
    let store = subscriptions.store.lock().unwrap();
    assert!(
        store.subscriptions[0]
            .skills
            .iter()
            .any(|skill| skill.id == "writer")
    );
    assert_eq!(
        store.subscriptions[0].remote_state.status,
        ChannelSubscriptionRemoteStatus::IntegrityError
    );
}

#[tokio::test]
async fn convert_to_local_uses_complete_copy_and_rejects_name_conflicts_without_untracking() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    let before = subscriptions.store.lock().unwrap().clone();
    installer
        .local_name_conflicts
        .lock()
        .unwrap()
        .insert("writer.local".into());

    let error = app
        .convert_removed_skill_to_local(ConvertRemovedChannelSkillRequest {
            repository_id: 42,
            skill_id: "writer".into(),
            local_name: "writer.local".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::SubscriptionUpdateFailed);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);

    let result = app
        .convert_removed_skill_to_local(ConvertRemovedChannelSkillRequest {
            repository_id: 42,
            skill_id: "writer".into(),
            local_name: "writer.archive.local".into(),
        })
        .await
        .unwrap();
    assert_eq!(result.local_name.as_deref(), Some("writer.archive.local"));
    assert_eq!(
        *installer.converted.lock().unwrap(),
        vec![("writer".into(), "writer.archive.local".into())]
    );
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );
}

#[tokio::test]
async fn reintroduced_skill_requires_explicit_install_and_does_not_replace_the_local_copy() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    app.convert_removed_skill_to_local(ConvertRemovedChannelSkillRequest {
        repository_id: 42,
        skill_id: "writer".into(),
        local_name: "writer.local".into(),
    })
    .await
    .unwrap();
    *gateway.manifests.lock().unwrap() =
        vec![manifest_v1(), removed_manifest(), reintroduced_manifest()];

    let checked = app.check_update(42).await.unwrap();
    let writer = checked
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.change, ChannelUpdateChange::Added);
    assert_eq!(writer.state, ChannelUpdateItemState::Notification);
    assert!(installer.install_requests.lock().unwrap().is_empty());

    *subscriptions.fail_save.lock().unwrap() = true;
    let error = app.install_channel_skill(42, "writer").await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert_eq!(*installer.install_rollbacks.lock().unwrap(), 1);
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );

    *subscriptions.fail_save.lock().unwrap() = false;
    let result = app.install_channel_skill(42, "writer").await.unwrap();
    assert!(
        result
            .subscription
            .skills
            .iter()
            .any(|skill| skill.id == "writer")
    );
    assert_eq!(installer.install_requests.lock().unwrap().len(), 2);
    assert_eq!(
        installer.install_requests.lock().unwrap()[1].selected_skill_ids,
        vec!["writer"]
    );
    assert_eq!(
        *installer.converted.lock().unwrap(),
        vec![("writer".into(), "writer.local".into())]
    );
}

#[tokio::test]
async fn invalid_reintroduced_install_rolls_back_before_a_frozen_state_save_failure() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    app.convert_removed_skill_to_local(ConvertRemovedChannelSkillRequest {
        repository_id: 42,
        skill_id: "writer".into(),
        local_name: "writer.local".into(),
    })
    .await
    .unwrap();
    *gateway.manifests.lock().unwrap() =
        vec![manifest_v1(), removed_manifest(), reintroduced_manifest()];
    let before = subscriptions.store.lock().unwrap().clone();
    *installer.invalid_install_receipt.lock().unwrap() = true;
    *installer.fail_store_after_install.lock().unwrap() = Some(subscriptions.fail_save.clone());

    let error = app.install_channel_skill(42, "writer").await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(error.message.contains("staged install was rolled back"));
    assert_eq!(*installer.install_rollbacks.lock().unwrap(), 1);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);
}

#[tokio::test]
async fn reintroduced_skill_stays_removed_until_the_pending_choice_is_resolved() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    *gateway.manifests.lock().unwrap() =
        vec![manifest_v1(), removed_manifest(), reintroduced_manifest()];

    let checked = app.check_update(42).await.unwrap();
    let writer = checked
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.state, ChannelUpdateItemState::RemovedFromChannel);
    assert!(installer.install_requests.lock().unwrap().is_empty());
    assert_eq!(
        app.install_channel_skill(42, "writer")
            .await
            .unwrap_err()
            .code,
        SharedChannelErrorCode::SubscriptionSelectionInvalid
    );

    let converted = app
        .convert_removed_skill_to_local(ConvertRemovedChannelSkillRequest {
            repository_id: 42,
            skill_id: "writer".into(),
            local_name: "writer.local".into(),
        })
        .await
        .unwrap();
    let writer = converted
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.state, ChannelUpdateItemState::Notification);
}

#[tokio::test]
async fn removal_rolls_back_before_commit_but_keeps_committed_cleanup_failures_untracked() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    prepare_removed_release(&gateway, &app).await;
    let before = subscriptions.store.lock().unwrap().clone();
    *subscriptions.fail_save.lock().unwrap() = true;

    let error = app.uninstall_removed_skill(42, "writer").await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);
    assert!(installer.uninstalled.lock().unwrap().is_empty());

    *subscriptions.fail_save.lock().unwrap() = false;
    installer
        .removal_cleanup_failures
        .lock()
        .unwrap()
        .insert("writer".into());
    let error = app.uninstall_removed_skill(42, "writer").await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::SubscriptionUpdateFailed);
    assert_eq!(*installer.uninstalled.lock().unwrap(), vec!["writer"]);
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .all(|skill| skill.id != "writer")
    );
}
