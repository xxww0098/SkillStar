use super::channel_update_tests::{fixtures, manifest_v1, service, target_v2};
use super::*;

async fn advance_to_v2(
    app: &ChannelSubscriptionFacade<
        super::channel_update_tests::UpdateGateway,
        super::channel_update_tests::UpdateChannels,
        super::channel_update_tests::UpdateSubscriptions,
        super::channel_update_tests::UpdateInstaller,
    >,
) {
    app.apply_update(ApplyChannelUpdateRequest {
        repository_id: 42,
        target: target_v2(),
        resolutions: Vec::new(),
    })
    .await
    .unwrap();
}

fn target_v1() -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: 1,
        tag_name: revision_tag(1),
        commit_sha: "a".repeat(40),
    }
}

#[tokio::test]
async fn lists_verified_older_targets_and_rolls_back_only_the_selected_skill() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway, subscriptions.clone(), installer.clone());
    advance_to_v2(&app).await;

    let targets = app.list_skill_rollback_targets(42, "reader").await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target, target_v1());
    assert_eq!(
        targets[0].content_hash,
        manifest_v1().skills[0].content_hash
    );

    let result = app
        .rollback_skill(RollbackChannelSkillRequest {
            repository_id: 42,
            skill_id: "reader".into(),
            target: target_v1(),
            resolution: None,
        })
        .await
        .unwrap();

    assert_eq!(result.pin.skill_id, "reader");
    assert_eq!(result.pin.target, target_v1());
    let store = subscriptions.store.lock().unwrap();
    let subscription = &store.subscriptions[0];
    assert_eq!(subscription.target, target_v2());
    assert_eq!(subscription.pins, vec![result.pin.clone()]);
    assert_eq!(
        subscription
            .skills
            .iter()
            .find(|skill| skill.id == "reader")
            .unwrap()
            .release_content_hash,
        manifest_v1().skills[0].content_hash
    );
    assert_eq!(
        subscription
            .skills
            .iter()
            .find(|skill| skill.id == "writer")
            .unwrap()
            .release_content_hash,
        super::channel_update_tests::manifest_v2().skills[1].content_hash
    );
    let reader = result
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "reader")
        .unwrap();
    assert_eq!(reader.pinned_target, Some(target_v1()));
    assert_eq!(reader.state, ChannelUpdateItemState::Available);
    assert_eq!(*installer.metadata_commits.lock().unwrap(), 1);
}

#[tokio::test]
async fn invalid_or_failed_history_keeps_the_current_version_and_does_not_pin() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    advance_to_v2(&app).await;
    let before = subscriptions.store.lock().unwrap().clone();

    let error = app
        .rollback_skill(RollbackChannelSkillRequest {
            repository_id: 42,
            skill_id: "reader".into(),
            target: ChannelReleaseTarget {
                commit_sha: "9".repeat(40),
                ..target_v1()
            },
            resolution: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);

    installer.failures.lock().unwrap().insert("reader".into());
    let error = app
        .rollback_skill(RollbackChannelSkillRequest {
            repository_id: 42,
            skill_id: "reader".into(),
            target: target_v1(),
            resolution: None,
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::SubscriptionUpdateFailed);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);
}

#[tokio::test]
async fn save_failure_rolls_back_the_staged_history_and_keeps_the_pin_absent() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway, subscriptions.clone(), installer.clone());
    advance_to_v2(&app).await;
    let before = subscriptions.store.lock().unwrap().clone();
    *subscriptions.fail_save.lock().unwrap() = true;

    let error = app
        .rollback_skill(RollbackChannelSkillRequest {
            repository_id: 42,
            skill_id: "reader".into(),
            target: target_v1(),
            resolution: None,
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert_eq!(*subscriptions.store.lock().unwrap(), before);
}

#[tokio::test]
async fn pin_survives_restart_suppresses_updates_and_resume_replans_latest() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    advance_to_v2(&app).await;
    app.rollback_skill(RollbackChannelSkillRequest {
        repository_id: 42,
        skill_id: "reader".into(),
        target: target_v1(),
        resolution: None,
    })
    .await
    .unwrap();
    drop(app);

    let restarted = service(gateway, subscriptions.clone(), installer);
    let applied = restarted
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    assert!(!applied.applied_skill_ids.iter().any(|id| id == "reader"));
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .pins
            .len(),
        1
    );

    let snapshot = restarted
        .resume_following_skill(42, "reader")
        .await
        .unwrap();
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .pins
            .is_empty()
    );
    let reader = snapshot
        .items
        .iter()
        .find(|item| item.id == "reader")
        .unwrap();
    assert_eq!(reader.pinned_target, None);
    assert_eq!(reader.state, ChannelUpdateItemState::Available);
}

#[tokio::test]
async fn unchanged_releases_use_the_subscription_target_to_resolve_history() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions, installer);
    advance_to_v2(&app).await;
    let mut third = super::channel_update_tests::manifest_v2();
    third.revision = 3;
    third.tag_name = revision_tag(3);
    third.published_at = "2026-08-07T00:00:00Z".into();
    third.title = "Release 3".into();
    third.notes = "No content changes".into();
    gateway.manifests.lock().unwrap().push(third.clone());
    app.apply_update(ApplyChannelUpdateRequest {
        repository_id: 42,
        target: ChannelReleaseTarget {
            revision: third.revision,
            tag_name: third.tag_name,
            commit_sha: third.commit_sha,
        },
        resolutions: Vec::new(),
    })
    .await
    .unwrap();

    let targets = app.list_skill_rollback_targets(42, "reader").await.unwrap();
    assert_eq!(targets[0].target, target_v2());
    assert_eq!(targets[1].target, target_v1());
}
