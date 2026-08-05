use super::channel_update_tests::{fixtures, manifest_v2, service};
use super::*;
use chrono::{DateTime, Duration, Utc};

fn instant(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc)
}

fn make_due(subscriptions: &super::channel_update_tests::UpdateSubscriptions, now: DateTime<Utc>) {
    let mut store = subscriptions.store.lock().unwrap();
    let last_run = store.subscriptions[0].auto_update.last_run.clone();
    store.subscriptions[0].auto_update = ChannelAutoUpdateState {
        enabled: true,
        next_check_at: Some(now.to_rfc3339()),
        last_run,
    };
}

#[tokio::test]
async fn auto_update_is_opt_in_and_the_preference_survives_restart() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer.clone());
    assert!(!app.auto_update_state(42).unwrap().enabled);
    let checked = app.run_due_auto_updates().await.unwrap();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0].run.status, ChannelAutoUpdateRunStatus::Checked);
    assert!(checked[0].run.applied_skill_ids.is_empty());
    assert!(checked[0].run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("newcomer")
            && pause.reason == ChannelAutoUpdatePauseReason::NewSkillRequiresReview
    }));
    assert!(installer.applied.lock().unwrap().is_empty());

    let enabled = app.set_auto_update_enabled(42, true).await.unwrap();
    assert!(enabled.enabled);
    assert!(enabled.next_check_at.is_some());

    let restarted = service(gateway, subscriptions, installer);
    assert!(restarted.auto_update_state(42).unwrap().enabled);
    let disabled = restarted.set_auto_update_enabled(42, false).await.unwrap();
    assert!(!disabled.enabled);
    assert!(disabled.next_check_at.is_some());
    assert!(restarted.run_due_auto_updates().await.unwrap().is_empty());
}

#[tokio::test]
async fn due_cycle_applies_clean_skill_and_pauses_divergent_and_new_items() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer.divergent.lock().unwrap().insert("writer".into());
    make_due(&subscriptions, now);
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let executions = app.run_due_auto_updates_at(now).await.unwrap();
    assert_eq!(executions.len(), 1);
    let run = &executions[0].run;
    assert_eq!(run.status, ChannelAutoUpdateRunStatus::PartiallyApplied);
    assert_eq!(run.applied_skill_ids, ["reader"]);
    assert!(run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("writer")
            && pause.reason == ChannelAutoUpdatePauseReason::LocalContentChanged
    }));
    assert!(run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("newcomer")
            && pause.reason == ChannelAutoUpdatePauseReason::NewSkillRequiresReview
    }));
    assert_eq!(installer.applied.lock().unwrap().len(), 1);
    assert_eq!(installer.applied.lock().unwrap()[0].installed.id, "reader");

    let stored = subscriptions.store.lock().unwrap().subscriptions[0].clone();
    assert_eq!(stored.target.revision, 1);
    assert_eq!(stored.auto_update.last_run.as_ref(), Some(run));
    assert!(
        app.run_due_auto_updates_at(now + Duration::minutes(30))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn pinned_removed_and_new_skills_are_never_applied_automatically() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    let mut release = manifest_v2();
    release
        .skills
        .iter_mut()
        .find(|skill| skill.id == "writer")
        .unwrap()
        .status = ChannelSkillReleaseStatus::Removed;
    *gateway.manifests.lock().unwrap() = vec![release];
    {
        let mut store = subscriptions.store.lock().unwrap();
        let subscription = &mut store.subscriptions[0];
        subscription.pins.push(ChannelSkillPin {
            skill_id: "reader".into(),
            target: subscription.target.clone(),
        });
    }
    make_due(&subscriptions, now);

    let execution = service(gateway, subscriptions, installer.clone())
        .run_due_auto_updates_at(now)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(execution.run.status, ChannelAutoUpdateRunStatus::Paused);
    assert!(execution.run.applied_skill_ids.is_empty());
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::Pinned
    }));
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("writer")
            && pause.reason == ChannelAutoUpdatePauseReason::RemovedUpstream
    }));
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("newcomer")
            && pause.reason == ChannelAutoUpdatePauseReason::NewSkillRequiresReview
    }));
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn baseline_snapshot_and_unresolved_failures_stay_paused() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer
        .divergent
        .lock()
        .unwrap()
        .extend(["reader".to_string(), "writer".to_string()]);
    installer.divergence_reasons.lock().unwrap().insert(
        "reader".into(),
        crate::skill_update::LocalDivergenceReason::BaselineMissing,
    );
    installer.divergence_reasons.lock().unwrap().insert(
        "writer".into(),
        crate::skill_update::LocalDivergenceReason::SnapshotFailed,
    );
    make_due(&subscriptions, now);

    let execution = service(gateway, subscriptions, installer)
        .run_due_auto_updates_at(now)
        .await
        .unwrap()
        .remove(0);
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::BaselineMissing
    }));
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("writer")
            && pause.reason == ChannelAutoUpdatePauseReason::SnapshotFailed
    }));

    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway, subscriptions.clone(), installer.clone());
    let mut previous = app.check_update(42).await.unwrap();
    let failed = previous
        .items
        .iter_mut()
        .find(|item| item.id == "reader")
        .unwrap();
    failed.state = ChannelUpdateItemState::Failed;
    failed.error = Some("previous apply failed".into());
    subscriptions.store.lock().unwrap().subscriptions[0].last_update = Some(previous);
    make_due(&subscriptions, now);

    let execution = app.run_due_auto_updates_at(now).await.unwrap().remove(0);
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));
    assert!(
        installer
            .applied
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.installed.id != "reader")
    );

    make_due(&subscriptions, now + Duration::hours(1));
    let still_paused = app
        .run_due_auto_updates_at(now + Duration::hours(1))
        .await
        .unwrap()
        .remove(0);
    assert!(still_paused.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));

    let target = app.update_state(42).unwrap().unwrap().target;
    app.apply_update(ApplyChannelUpdateRequest {
        repository_id: 42,
        target,
        resolutions: Vec::new(),
    })
    .await
    .unwrap();
    make_due(&subscriptions, now + Duration::hours(2));
    let recovered = app
        .run_due_auto_updates_at(now + Duration::hours(2))
        .await
        .unwrap()
        .remove(0);
    assert!(!recovered.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));
}

#[tokio::test]
async fn inspection_error_pauses_only_that_skill() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer
        .inspection_failures
        .lock()
        .unwrap()
        .insert("reader".into());
    make_due(&subscriptions, now);

    let execution = service(gateway, subscriptions, installer)
        .run_due_auto_updates_at(now)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(execution.run.applied_skill_ids, ["writer"]);
    assert!(execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::SnapshotFailed
    }));
}

#[tokio::test]
async fn retryable_item_failure_retries_without_blocking_clean_skills() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("reader".into());
    installer
        .failure_codes
        .lock()
        .unwrap()
        .insert("reader".into(), SharedChannelErrorCode::Network);
    make_due(&subscriptions, now);
    let app = service(gateway, subscriptions.clone(), installer.clone());

    let first = app.run_due_auto_updates_at(now).await.unwrap().remove(0);
    assert_eq!(
        first.run.status,
        ChannelAutoUpdateRunStatus::PartiallyApplied
    );
    assert!(first.run.retryable);
    assert_eq!(first.run.applied_skill_ids, ["writer"]);
    assert!(!first.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .auto_update
            .next_check_at
            .as_deref(),
        Some("2026-08-06T01:05:00+00:00")
    );

    installer.failures.lock().unwrap().clear();
    let retried = app
        .run_due_auto_updates_at(now + Duration::minutes(5))
        .await
        .unwrap()
        .remove(0);
    assert!(
        retried
            .run
            .applied_skill_ids
            .iter()
            .any(|id| id == "reader")
    );
}

#[tokio::test]
async fn cancelled_item_fetch_is_not_persisted_as_an_unresolved_failure() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("reader".into());
    installer
        .failure_codes
        .lock()
        .unwrap()
        .insert("reader".into(), SharedChannelErrorCode::Cancelled);
    make_due(&subscriptions, now);

    let execution = service(gateway, subscriptions.clone(), installer)
        .run_due_auto_updates_at(now)
        .await
        .unwrap()
        .remove(0);
    assert!(execution.run.retryable);
    assert!(!execution.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .auto_update
            .next_check_at
            .as_deref(),
        Some("2026-08-06T01:05:00+00:00")
    );
}

#[tokio::test]
async fn per_item_permission_reason_survives_later_checks() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("reader".into());
    installer
        .failure_codes
        .lock()
        .unwrap()
        .insert("reader".into(), SharedChannelErrorCode::PermissionDenied);
    make_due(&subscriptions, now);
    let app = service(gateway, subscriptions.clone(), installer);

    let first = app.run_due_auto_updates_at(now).await.unwrap().remove(0);
    assert!(first.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::PermissionChanged
    }));
    make_due(&subscriptions, now + Duration::hours(1));
    let second = app
        .run_due_auto_updates_at(now + Duration::hours(1))
        .await
        .unwrap()
        .remove(0);
    assert!(second.run.pauses.iter().any(|pause| {
        pause.skill_id.as_deref() == Some("reader")
            && pause.reason == ChannelAutoUpdatePauseReason::PermissionChanged
    }));
}

#[tokio::test]
async fn fatal_apply_failure_hard_pauses_until_manual_intervention() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    installer
        .verification_failures
        .lock()
        .unwrap()
        .insert("reader".into());
    installer
        .rollback_failures
        .lock()
        .unwrap()
        .insert("reader".into());
    make_due(&subscriptions, now);
    let app = service(gateway, subscriptions.clone(), installer);

    let failed = app.run_due_auto_updates_at(now).await.unwrap().remove(0);
    assert_eq!(failed.run.status, ChannelAutoUpdateRunStatus::Paused);
    assert!(failed.run.pauses.iter().any(|pause| {
        pause.skill_id.is_none() && pause.reason == ChannelAutoUpdatePauseReason::UnresolvedFailure
    }));
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .auto_update
            .next_check_at
            .is_none()
    );
    assert!(
        app.run_due_auto_updates_at(now + Duration::hours(24))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn stale_claim_token_cannot_apply_after_a_new_owner_reclaims() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut store = subscriptions.store.lock().unwrap();
        store.subscriptions[0].auto_update.enabled = true;
        store.subscriptions[0].auto_update.last_run = Some(ChannelAutoUpdateRun::checking(now));
    }
    let app = service(gateway, subscriptions, installer.clone());
    let error = app
        .apply_update_selected(
            ApplyChannelUpdateRequest {
                repository_id: 42,
                target: super::channel_update_tests::target_v2(),
                resolutions: Vec::new(),
            },
            None,
            true,
            Some("2026-08-06T00:00:00+00:00"),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Cancelled);
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn active_claim_guard_prevents_timeout_or_toggle_from_reclaiming() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    make_due(&subscriptions, now);
    let app = service(gateway, subscriptions, installer);

    let claims = app.claim_due_auto_updates(now).await.unwrap();
    assert_eq!(claims.len(), 1);
    app.set_auto_update_enabled(42, false).await.unwrap();
    app.set_auto_update_enabled(42, true).await.unwrap();
    assert!(
        app.run_due_auto_updates_at(now + Duration::hours(24))
            .await
            .unwrap()
            .is_empty()
    );
    drop(claims);
    assert_eq!(
        app.run_due_auto_updates_at(now + Duration::hours(24))
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn completion_clock_schedules_the_next_check_from_finished_time() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let started = instant("2026-08-06T01:00:00Z");
    let finished = instant("2026-08-06T02:30:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    make_due(&subscriptions, started);
    let app = service(gateway, subscriptions.clone(), installer);
    let calls = Arc::new(AtomicUsize::new(0));
    let executions = app
        .run_due_auto_updates_with({
            let calls = Arc::clone(&calls);
            move || {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                    started
                } else {
                    finished
                }
            }
        })
        .await
        .unwrap();

    assert_eq!(
        executions[0].run.completed_at.as_deref(),
        Some("2026-08-06T02:30:00+00:00")
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .auto_update
            .next_check_at
            .as_deref(),
        Some("2026-08-06T03:30:00+00:00")
    );
}

#[tokio::test]
async fn concurrent_due_scans_claim_a_channel_once() {
    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    make_due(&subscriptions, now);
    let first = service(gateway.clone(), subscriptions.clone(), installer.clone());
    let second = service(gateway, subscriptions, installer.clone());

    let (first_result, second_result) = tokio::join!(
        first.run_due_auto_updates_at(now),
        second.run_due_auto_updates_at(now)
    );
    let total = first_result.unwrap().len() + second_result.unwrap().len();
    assert_eq!(total, 1);
    let applied = installer.applied.lock().unwrap();
    assert_eq!(
        applied
            .iter()
            .filter(|request| request.installed.id == "reader")
            .count(),
        1
    );
    assert_eq!(
        applied
            .iter()
            .filter(|request| request.installed.id == "writer")
            .count(),
        1
    );
}

#[tokio::test]
async fn interrupted_checking_run_is_reclaimed_after_restart_timeout() {
    let started = instant("2026-08-06T01:00:00Z");
    let retry_at = started + Duration::minutes(5);
    let (gateway, subscriptions, installer) = fixtures();
    {
        let mut store = subscriptions.store.lock().unwrap();
        store.subscriptions[0].auto_update = ChannelAutoUpdateState {
            enabled: true,
            next_check_at: Some((started + Duration::hours(1)).to_rfc3339()),
            last_run: Some(ChannelAutoUpdateRun {
                started_at: started.to_rfc3339(),
                completed_at: None,
                status: ChannelAutoUpdateRunStatus::Checking,
                target: None,
                applied_skill_ids: Vec::new(),
                pauses: Vec::new(),
                error: None,
                retryable: false,
            }),
        };
    }
    let restarted = service(gateway, subscriptions.clone(), installer);
    assert!(
        restarted
            .run_due_auto_updates_at(retry_at - Duration::seconds(1))
            .await
            .unwrap()
            .is_empty()
    );
    let execution = restarted
        .run_due_auto_updates_at(retry_at)
        .await
        .unwrap()
        .remove(0);
    assert_ne!(execution.run.status, ChannelAutoUpdateRunStatus::Checking);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .auto_update
            .last_run
            .as_ref(),
        Some(&execution.run)
    );
}

#[tokio::test]
async fn fake_gateway_distinguishes_permission_integrity_and_retryable_network_failures() {
    for (code, status, reason) in [
        (
            SharedChannelErrorCode::PermissionDenied,
            ChannelAutoUpdateRunStatus::Paused,
            Some(ChannelAutoUpdatePauseReason::PermissionChanged),
        ),
        (
            SharedChannelErrorCode::Integrity,
            ChannelAutoUpdateRunStatus::Paused,
            Some(ChannelAutoUpdatePauseReason::IntegrityError),
        ),
        (
            SharedChannelErrorCode::Network,
            ChannelAutoUpdateRunStatus::RetryableFailure,
            None,
        ),
    ] {
        let now = instant("2026-08-06T01:00:00Z");
        let (gateway, subscriptions, installer) = fixtures();
        *gateway.repository_error.lock().unwrap() = Some(code);
        make_due(&subscriptions, now);
        let execution = service(gateway, subscriptions.clone(), installer)
            .run_due_auto_updates_at(now)
            .await
            .unwrap()
            .remove(0);
        assert_eq!(execution.run.status, status);
        assert_eq!(
            execution.run.pauses.first().map(|pause| pause.reason),
            reason
        );
        if code == SharedChannelErrorCode::Network {
            assert_eq!(
                subscriptions.store.lock().unwrap().subscriptions[0]
                    .auto_update
                    .next_check_at
                    .as_deref(),
                Some("2026-08-06T01:05:00+00:00")
            );
        }
    }

    let now = instant("2026-08-06T01:00:00Z");
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    app.check_update(42).await.unwrap();
    gateway.manifests.lock().unwrap().truncate(1);
    make_due(&subscriptions, now);
    let conflict = app.run_due_auto_updates_at(now).await.unwrap().remove(0);
    assert_eq!(conflict.run.status, ChannelAutoUpdateRunStatus::Paused);
    assert_eq!(
        conflict.run.pauses[0].reason,
        ChannelAutoUpdatePauseReason::IntegrityError
    );
    let stored = subscriptions.store.lock().unwrap().subscriptions[0]
        .last_update
        .clone()
        .unwrap();
    assert_eq!(
        stored.check_error_code,
        Some(SharedChannelErrorCode::ReleaseConflict)
    );
}
