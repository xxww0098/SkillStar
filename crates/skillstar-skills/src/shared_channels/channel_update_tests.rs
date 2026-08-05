use super::*;
use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct UpdateGateway {
    pub(super) manifests: Arc<Mutex<Vec<ChannelReleaseManifest>>>,
    pub(super) offline: Arc<Mutex<bool>>,
    pub(super) repository_error: Arc<Mutex<Option<SharedChannelErrorCode>>>,
}

#[async_trait]
impl ChannelSubscriptionGateway for UpdateGateway {
    async fn accessible_repository(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        if *self.offline.lock().unwrap() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Network,
                "offline",
            ));
        }
        if let Some(code) = *self.repository_error.lock().unwrap() {
            return Err(SharedChannelError::new(code, "repository unavailable"));
        }
        if repository_id == 42 {
            Ok(repository())
        } else {
            Err(SharedChannelError::new(
                SharedChannelErrorCode::RepositoryNotFound,
                "missing",
            ))
        }
    }

    async fn published_manifests(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<Vec<ChannelReleaseManifest>, SharedChannelError> {
        Ok(self.manifests.lock().unwrap().clone())
    }
}

#[derive(Clone)]
pub(super) struct UpdateChannels(Arc<Mutex<SharedChannelStore>>);

#[async_trait]
impl SharedChannelRegistry for UpdateChannels {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        *self.0.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct UpdateSubscriptions {
    pub(super) store: Arc<Mutex<ChannelSubscriptionStore>>,
    pub(super) fail_save: Arc<Mutex<bool>>,
}

impl ChannelSubscriptionRegistry for UpdateSubscriptions {
    fn auto_update_scope_key(&self) -> String {
        format!("update-test:{:p}", Arc::as_ptr(&self.store))
    }

    fn list_views(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .subscriptions
            .iter()
            .map(ChannelSubscriptionView::from_subscription)
            .collect())
    }

    fn load_mutable(&self) -> Result<ChannelSubscriptionStore, SharedChannelError> {
        Ok(self.store.lock().unwrap().clone())
    }

    fn save(&self, store: &ChannelSubscriptionStore) -> Result<(), SharedChannelError> {
        if *self.fail_save.lock().unwrap() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                "save failed",
            ));
        }
        *self.store.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(super) struct UpdateInstaller {
    pub(super) divergent: Arc<Mutex<BTreeSet<String>>>,
    pub(super) divergence_reasons:
        Arc<Mutex<BTreeMap<String, crate::skill_update::LocalDivergenceReason>>>,
    pub(super) failures: Arc<Mutex<BTreeSet<String>>>,
    pub(super) failure_codes: Arc<Mutex<BTreeMap<String, SharedChannelErrorCode>>>,
    pub(super) inspection_failures: Arc<Mutex<BTreeSet<String>>>,
    inspection_failures_after_apply: Arc<Mutex<BTreeSet<String>>>,
    pub(super) verification_failures: Arc<Mutex<BTreeSet<String>>>,
    pub(super) rollback_failures: Arc<Mutex<BTreeSet<String>>>,
    pub(super) applied: Arc<Mutex<Vec<ChannelSkillUpdateRequest>>>,
    rollbacks: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ChannelSubscriptionInstaller for UpdateInstaller {
    async fn install(
        &self,
        _request: ChannelInstallRequest,
    ) -> Result<ChannelInstallReceipt, SharedChannelError> {
        unreachable!("update tests do not install a new subscription")
    }

    async fn rollback(&self, _receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError> {
        unreachable!("update tests use update rollback receipts")
    }
}

#[async_trait]
impl ChannelSubscriptionUpdater for UpdateInstaller {
    async fn inspect(
        &self,
        skill: &ChannelSubscribedSkill,
    ) -> Result<ChannelUpdateInspection, SharedChannelError> {
        if self
            .inspection_failures
            .lock()
            .unwrap()
            .contains(&skill.id.to_ascii_lowercase())
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("{} snapshot failed", skill.id),
            ));
        }
        if !self.applied.lock().unwrap().is_empty()
            && self
                .inspection_failures_after_apply
                .lock()
                .unwrap()
                .contains(&skill.id.to_ascii_lowercase())
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("{} inspection failed", skill.id),
            ));
        }
        if self
            .divergent
            .lock()
            .unwrap()
            .contains(&skill.id.to_ascii_lowercase())
        {
            Ok(ChannelUpdateInspection::Divergent {
                reason: self
                    .divergence_reasons
                    .lock()
                    .unwrap()
                    .get(&skill.id.to_ascii_lowercase())
                    .copied()
                    .unwrap_or(crate::skill_update::LocalDivergenceReason::ContentChanged),
                suggested_local_name: format!("{}.local", skill.id),
                error: None,
            })
        } else {
            Ok(ChannelUpdateInspection::Clean)
        }
    }

    async fn apply(
        &self,
        request: ChannelSkillUpdateRequest,
    ) -> Result<ChannelSkillUpdateReceipt, SharedChannelError> {
        let key = request.installed.id.to_ascii_lowercase();
        self.applied.lock().unwrap().push(request.clone());
        if self.failures.lock().unwrap().contains(&key) {
            return Err(SharedChannelError::new(
                self.failure_codes
                    .lock()
                    .unwrap()
                    .get(&key)
                    .copied()
                    .unwrap_or(SharedChannelErrorCode::SubscriptionUpdateFailed),
                format!("{key} failed"),
            ));
        }
        if self.divergent.lock().unwrap().contains(&key) && request.resolution.is_none() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("{key} is divergent"),
            ));
        }
        self.divergent.lock().unwrap().remove(&key);
        let installed = subscribed_skill(&request.released, &request.manifest);
        Ok(ChannelSkillUpdateReceipt {
            previous: request.installed,
            installed,
            previous_checkout: "/fake/checkout".into(),
            previous_lock_entry: lock_entry("previous", "a"),
            previous_update_available: None,
            update_state_revision_after_apply: None,
        })
    }

    async fn verify(&self, receipt: &ChannelSkillUpdateReceipt) -> Result<(), SharedChannelError> {
        if self
            .verification_failures
            .lock()
            .unwrap()
            .contains(&receipt.installed.id.to_ascii_lowercase())
        {
            Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("{} changed after apply", receipt.installed.id),
            ))
        } else {
            Ok(())
        }
    }

    async fn rollback(
        &self,
        receipt: &ChannelSkillUpdateReceipt,
    ) -> Result<(), SharedChannelError> {
        self.rollbacks
            .lock()
            .unwrap()
            .push(receipt.previous.id.clone());
        if self
            .rollback_failures
            .lock()
            .unwrap()
            .contains(&receipt.previous.id.to_ascii_lowercase())
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionUpdateFailed,
                format!("{} rollback failed", receipt.previous.id),
            ));
        }
        Ok(())
    }
}

pub(super) fn service(
    gateway: UpdateGateway,
    subscriptions: UpdateSubscriptions,
    installer: UpdateInstaller,
) -> ChannelSubscriptionFacade<UpdateGateway, UpdateChannels, UpdateSubscriptions, UpdateInstaller>
{
    ChannelSubscriptionFacade::new(
        gateway,
        UpdateChannels(Arc::new(Mutex::new(SharedChannelStore {
            schema_version: SHARED_CHANNEL_STORE_VERSION,
            channels: vec![channel()],
        }))),
        subscriptions,
        installer,
    )
}

pub(super) fn fixtures() -> (UpdateGateway, UpdateSubscriptions, UpdateInstaller) {
    let gateway = UpdateGateway {
        manifests: Arc::new(Mutex::new(vec![manifest_v1(), manifest_v2()])),
        offline: Arc::new(Mutex::new(false)),
        repository_error: Arc::new(Mutex::new(None)),
    };
    let subscriptions = UpdateSubscriptions {
        store: Arc::new(Mutex::new(ChannelSubscriptionStore {
            schema_version: CHANNEL_SUBSCRIPTION_STORE_VERSION,
            subscriptions: vec![subscription_v1()],
        })),
        fail_save: Arc::new(Mutex::new(false)),
    };
    (gateway, subscriptions, UpdateInstaller::default())
}

#[tokio::test]
async fn clean_skills_advance_independently_and_restart_keeps_the_result() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway, subscriptions.clone(), installer.clone());
    let checked = app.check_update(42).await.unwrap();
    assert_eq!(checked.target.revision, 2);
    assert_eq!(checked.status, ChannelUpdateStatus::UpdateAvailable);
    assert_eq!(
        checked
            .items
            .iter()
            .map(|item| (&item.id, item.change, item.state))
            .collect::<Vec<_>>(),
        vec![
            (
                &"newcomer".to_string(),
                ChannelUpdateChange::Added,
                ChannelUpdateItemState::Notification,
            ),
            (
                &"reader".to_string(),
                ChannelUpdateChange::Updated,
                ChannelUpdateItemState::Available,
            ),
            (
                &"writer".to_string(),
                ChannelUpdateChange::Updated,
                ChannelUpdateItemState::Available,
            ),
        ]
    );

    let result = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(result.applied_skill_ids, vec!["reader", "writer"]);
    assert_eq!(result.snapshot.status, ChannelUpdateStatus::UpToDate);
    assert!(result.snapshot.items.iter().all(|item| {
        item.change == ChannelUpdateChange::Updated && item.state == ChannelUpdateItemState::Applied
    }));
    assert_eq!(installer.applied.lock().unwrap().len(), 2);
    assert!(
        installer
            .applied
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.released.id != "newcomer")
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        2
    );

    let restarted = service(
        UpdateGateway {
            manifests: Arc::new(Mutex::new(vec![manifest_v2()])),
            offline: Arc::new(Mutex::new(false)),
            repository_error: Arc::new(Mutex::new(None)),
        },
        subscriptions,
        UpdateInstaller::default(),
    );
    assert_eq!(restarted.update_state(42).unwrap(), Some(result.snapshot));
}

#[tokio::test]
async fn divergent_skill_stays_old_while_clean_skill_advances() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.divergent.lock().unwrap().insert("writer".into());
    let result = service(gateway, subscriptions.clone(), installer)
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.applied_skill_ids, vec!["reader"]);
    assert_eq!(
        result.snapshot.status,
        ChannelUpdateStatus::PartiallyUpgraded
    );
    let writer = result
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.state, ChannelUpdateItemState::Blocked);
    assert_eq!(writer.suggested_local_name.as_deref(), Some("writer.local"));
    let persisted = &subscriptions.store.lock().unwrap().subscriptions[0];
    assert_eq!(persisted.target.revision, 1);
    assert_eq!(
        persisted
            .skills
            .iter()
            .find(|skill| skill.id == "writer")
            .unwrap()
            .release_content_hash,
        hash('c')
    );
}

#[tokio::test]
async fn explicit_local_resolution_allows_the_blocked_skill_to_advance() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.divergent.lock().unwrap().insert("writer".into());
    let result = service(gateway, subscriptions, installer.clone())
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: vec![ChannelSkillUpdateResolution {
                skill_id: "writer".into(),
                resolution: crate::skill_update::LocalDivergenceResolution::Preserve {
                    local_name: "writer.local".into(),
                },
            }],
        })
        .await
        .unwrap();

    assert_eq!(result.applied_skill_ids, vec!["reader", "writer"]);
    let applied = installer.applied.lock().unwrap();
    assert!(
        applied
            .iter()
            .find(|request| request.installed.id == "reader")
            .unwrap()
            .resolution
            .is_none()
    );
    let writer = applied
        .iter()
        .find(|request| request.installed.id == "writer")
        .unwrap();
    assert!(matches!(
        writer.resolution,
        Some(crate::skill_update::LocalDivergenceResolution::Preserve { .. })
    ));
}

#[tokio::test]
async fn local_resolution_is_rejected_for_a_skill_that_is_not_blocked() {
    let (gateway, subscriptions, installer) = fixtures();
    let error = service(gateway, subscriptions, installer.clone())
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: vec![ChannelSkillUpdateResolution {
                skill_id: "reader".into(),
                resolution: crate::skill_update::LocalDivergenceResolution::Discard,
            }],
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionSelectionInvalid
    );
    assert!(installer.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn failed_skill_is_reported_without_reverting_another_success() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("writer".into());
    let result = service(gateway, subscriptions.clone(), installer)
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.applied_skill_ids, vec!["reader"]);
    assert_eq!(
        result.snapshot.status,
        ChannelUpdateStatus::PartiallyUpgraded
    );
    let writer = result
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.state, ChannelUpdateItemState::Failed);
    assert_eq!(writer.error.as_deref(), Some("writer failed"));
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        1
    );
}

#[tokio::test]
async fn final_verification_rolls_back_only_the_skill_that_changed_after_apply() {
    let (gateway, subscriptions, installer) = fixtures();
    installer
        .verification_failures
        .lock()
        .unwrap()
        .insert("reader".into());
    let result = service(gateway, subscriptions, installer.clone())
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.applied_skill_ids, ["writer"]);
    assert_eq!(
        result.snapshot.status,
        ChannelUpdateStatus::PartiallyUpgraded
    );
    let reader = result
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "reader")
        .unwrap();
    assert_eq!(reader.state, ChannelUpdateItemState::Failed);
    assert_eq!(reader.error.as_deref(), Some("reader changed after apply"));
    assert_eq!(installer.rollbacks.lock().unwrap().as_slice(), ["reader"]);
}

#[tokio::test]
async fn storage_failure_rolls_back_every_skill_that_advanced() {
    let (gateway, subscriptions, installer) = fixtures();
    *subscriptions.fail_save.lock().unwrap() = true;
    let error = service(gateway, subscriptions, installer.clone())
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert_eq!(
        installer.rollbacks.lock().unwrap().as_slice(),
        ["writer", "reader"]
    );
}

#[tokio::test]
async fn post_apply_inspection_failure_does_not_roll_back_successful_skills() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.failures.lock().unwrap().insert("writer".into());
    installer
        .inspection_failures_after_apply
        .lock()
        .unwrap()
        .insert("writer".into());
    let result = service(gateway, subscriptions, installer.clone())
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.applied_skill_ids, ["reader"]);
    let writer = result
        .snapshot
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();
    assert_eq!(writer.state, ChannelUpdateItemState::Failed);
    assert_eq!(writer.error.as_deref(), Some("writer failed"));
    assert!(installer.rollbacks.lock().unwrap().is_empty());
}

#[tokio::test]
async fn offline_check_keeps_last_verified_target_and_persists_retryable_error() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    let verified = app.check_update(42).await.unwrap();
    *gateway.offline.lock().unwrap() = true;

    let offline = app.check_update(42).await.unwrap();
    assert_eq!(offline.target, verified.target);
    assert_eq!(offline.check_error.as_deref(), Some("offline"));
    let stored = subscriptions.store.lock().unwrap().subscriptions[0]
        .last_update
        .clone()
        .unwrap();
    assert_eq!(stored.target, verified.target);
    assert_eq!(stored.check_error.as_deref(), Some("offline"));
    assert_eq!(
        stored.check_error_code,
        Some(SharedChannelErrorCode::Network)
    );
}

#[tokio::test]
async fn removed_upstream_skill_is_blocked_and_kept_installed() {
    let (gateway, subscriptions, installer) = fixtures();
    let without_writer = manifest(
        2,
        'd',
        vec![release_skill("reader", 'e'), release_skill("newcomer", '9')],
    );
    *gateway.manifests.lock().unwrap() = vec![manifest_v1(), without_writer];

    let checked = service(gateway, subscriptions.clone(), installer.clone())
        .check_update(42)
        .await
        .unwrap();
    let writer = checked
        .items
        .iter()
        .find(|item| item.id == "writer")
        .unwrap();

    assert_eq!(writer.change, ChannelUpdateChange::Removed);
    assert_eq!(writer.state, ChannelUpdateItemState::Blocked);
    assert_eq!(
        writer.block_reason,
        Some(ChannelUpdateBlockReason::RemovedUpstream)
    );
    assert!(installer.applied.lock().unwrap().is_empty());
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .iter()
            .any(|skill| skill.id == "writer")
    );
}

#[tokio::test]
async fn unchanged_skill_does_not_make_an_initial_check_partially_upgraded() {
    let (gateway, subscriptions, installer) = fixtures();
    *gateway.manifests.lock().unwrap() = vec![
        manifest_v1(),
        manifest(
            2,
            'd',
            vec![release_skill("reader", 'b'), release_skill("writer", 'f')],
        ),
    ];

    let checked = service(gateway, subscriptions, installer)
        .check_update(42)
        .await
        .unwrap();

    assert_eq!(checked.status, ChannelUpdateStatus::UpdateAvailable);
    assert_eq!(
        checked
            .items
            .iter()
            .find(|item| item.id == "reader")
            .unwrap()
            .state,
        ChannelUpdateItemState::Current
    );
}

#[tokio::test]
async fn unchanged_skill_with_a_blocked_peer_persists_as_blocked() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.divergent.lock().unwrap().insert("writer".into());
    *gateway.manifests.lock().unwrap() = vec![
        manifest_v1(),
        manifest(
            2,
            'd',
            vec![release_skill("reader", 'b'), release_skill("writer", 'f')],
        ),
    ];

    let checked = service(gateway, subscriptions.clone(), installer)
        .check_update(42)
        .await
        .unwrap();

    assert_eq!(checked.status, ChannelUpdateStatus::Blocked);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0].last_update,
        Some(checked)
    );
}

#[tokio::test]
async fn added_only_release_can_be_acknowledged_without_installing_the_new_skill() {
    let (gateway, subscriptions, installer) = fixtures();
    let mut newcomer = release_skill("newcomer", '9');
    newcomer.status = ChannelSkillReleaseStatus::Added;
    *gateway.manifests.lock().unwrap() = vec![
        manifest_v1(),
        manifest(
            2,
            'd',
            vec![
                release_skill("reader", 'b'),
                release_skill("writer", 'c'),
                newcomer,
            ],
        ),
    ];
    let app = service(gateway, subscriptions.clone(), installer.clone());
    let checked = app.check_update(42).await.unwrap();
    assert_eq!(checked.status, ChannelUpdateStatus::UpdateAvailable);
    assert_eq!(
        checked
            .items
            .iter()
            .find(|item| item.id == "newcomer")
            .unwrap()
            .state,
        ChannelUpdateItemState::Notification
    );

    let acknowledged = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();

    assert!(acknowledged.applied_skill_ids.is_empty());
    assert_eq!(acknowledged.snapshot.status, ChannelUpdateStatus::UpToDate);
    assert!(
        acknowledged
            .snapshot
            .items
            .iter()
            .all(|item| item.id != "newcomer")
    );
    assert!(installer.applied.lock().unwrap().is_empty());
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        2
    );
}

#[tokio::test]
async fn unselected_skills_from_the_subscribed_release_are_not_reported_as_new() {
    let (gateway, subscriptions, installer) = fixtures();
    gateway.manifests.lock().unwrap().truncate(1);
    subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .retain(|skill| skill.id == "writer");

    let checked = service(gateway, subscriptions, installer)
        .check_update(42)
        .await
        .unwrap();

    assert_eq!(checked.status, ChannelUpdateStatus::UpToDate);
    assert_eq!(
        checked
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["writer"]
    );
}

#[tokio::test]
async fn a_skill_added_in_a_skipped_release_is_still_reported_as_new() {
    let (gateway, subscriptions, installer) = fixtures();
    *gateway.manifests.lock().unwrap() = vec![
        manifest_v1(),
        manifest(
            3,
            '7',
            vec![
                release_skill("reader", 'e'),
                release_skill("writer", 'f'),
                release_skill("newcomer", '8'),
            ],
        ),
    ];

    let checked = service(gateway, subscriptions, installer)
        .check_update(42)
        .await
        .unwrap();

    let newcomer = checked
        .items
        .iter()
        .find(|item| item.id == "newcomer")
        .unwrap();
    assert_eq!(newcomer.change, ChannelUpdateChange::Added);
    assert_eq!(newcomer.state, ChannelUpdateItemState::Notification);
}

#[tokio::test]
async fn empty_release_can_be_acknowledged_by_an_empty_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    subscriptions.store.lock().unwrap().subscriptions[0]
        .skills
        .clear();
    *gateway.manifests.lock().unwrap() = vec![manifest_v1(), manifest(2, 'd', Vec::new())];
    let app = service(gateway, subscriptions.clone(), installer);

    let checked = app.check_update(42).await.unwrap();
    assert!(checked.items.is_empty());
    assert!(checked.acknowledgement_required);
    assert_eq!(checked.status, ChannelUpdateStatus::UpdateAvailable);

    let acknowledged = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    assert!(!acknowledged.snapshot.acknowledgement_required);
    assert_eq!(acknowledged.snapshot.status, ChannelUpdateStatus::UpToDate);
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        2
    );
}

#[tokio::test]
async fn subscribed_skill_identity_casing_cannot_change() {
    let (gateway, subscriptions, installer) = fixtures();
    *gateway.manifests.lock().unwrap() = vec![
        manifest_v1(),
        manifest(
            2,
            'd',
            vec![release_skill("reader", 'e'), release_skill("Writer", 'f')],
        ),
    ];

    let error = service(gateway, subscriptions, installer)
        .check_update(42)
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
    assert!(error.message.contains("identity casing changed"));
}

#[tokio::test]
async fn a_missing_newer_release_never_downgrades_the_subscription() {
    let (gateway, subscriptions, installer) = fixtures();
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    let applied = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    *gateway.manifests.lock().unwrap() = vec![manifest_v1()];

    let checked = app.check_update(42).await.unwrap();
    assert_eq!(checked.target, applied.snapshot.target);
    assert!(
        checked
            .check_error
            .unwrap()
            .contains("refusing to downgrade")
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        2
    );
    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: ChannelReleaseTarget {
                revision: 1,
                tag_name: revision_tag(1),
                commit_sha: "a".repeat(40),
            },
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
}

#[tokio::test]
async fn a_partially_upgraded_subscription_never_downgrades_an_advanced_skill() {
    let (gateway, subscriptions, installer) = fixtures();
    installer.divergent.lock().unwrap().insert("writer".into());
    let app = service(gateway.clone(), subscriptions.clone(), installer);
    let partial = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: target_v2(),
            resolutions: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(
        partial.snapshot.status,
        ChannelUpdateStatus::PartiallyUpgraded
    );
    assert_eq!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .target
            .revision,
        1
    );
    *gateway.manifests.lock().unwrap() = vec![manifest_v1()];

    let checked = app.check_update(42).await.unwrap();
    assert_eq!(checked.target.revision, 2);
    assert!(
        checked
            .check_error
            .unwrap()
            .contains("refusing to downgrade")
    );
    let error = app
        .apply_update(ApplyChannelUpdateRequest {
            repository_id: 42,
            target: ChannelReleaseTarget {
                revision: 1,
                tag_name: revision_tag(1),
                commit_sha: "a".repeat(40),
            },
            resolutions: Vec::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
}

fn subscription_v1() -> ChannelSubscription {
    let manifest = manifest_v1();
    ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        target: ChannelReleaseTarget {
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: "a".repeat(40),
        },
        skills: manifest
            .skills
            .iter()
            .map(|skill| subscribed_skill(skill, &manifest))
            .collect(),
        known_skill_ids: vec!["reader".into(), "writer".into()],
        pins: Vec::new(),
        last_update: None,
        auto_update: ChannelAutoUpdateState::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn subscribed_skill(
    skill: &ChannelReleaseSkill,
    manifest: &ChannelReleaseManifest,
) -> ChannelSubscribedSkill {
    ChannelSubscribedSkill {
        id: skill.id.clone(),
        content_root: skill.content_root.clone(),
        release_content_hash: skill.content_hash.clone(),
        release_content_hash_version: skill.content_hash_version,
        baseline_hash: skill.content_hash.clone(),
        baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: 42,
            repository_url: "https://github.com/acme/channel.git".into(),
            git_ref: manifest.commit_sha.clone(),
            source_folder: skill.content_root.clone(),
        },
    }
}

fn manifest_v1() -> ChannelReleaseManifest {
    manifest(
        1,
        'a',
        vec![release_skill("reader", 'b'), release_skill("writer", 'c')],
    )
}

pub(super) fn manifest_v2() -> ChannelReleaseManifest {
    let mut newcomer = release_skill("newcomer", '9');
    newcomer.status = ChannelSkillReleaseStatus::Added;
    manifest(
        2,
        'd',
        vec![
            release_skill("reader", 'e'),
            release_skill("writer", 'f'),
            newcomer,
        ],
    )
}

fn manifest(
    revision: u64,
    commit: char,
    skills: Vec<ChannelReleaseSkill>,
) -> ChannelReleaseManifest {
    ChannelReleaseManifest {
        schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
        repository_id: 42,
        organization_id: 7,
        revision,
        tag_name: revision_tag(revision),
        commit_sha: commit.to_string().repeat(40),
        publisher: ChannelPublisherIdentity {
            id: 9,
            login: "alice".into(),
        },
        published_at: format!("2026-08-0{revision}T00:00:00Z"),
        title: format!("Release {revision}"),
        notes: format!("Notes {revision}"),
        skills,
    }
}

fn release_skill(id: &str, digest: char) -> ChannelReleaseSkill {
    ChannelReleaseSkill {
        id: id.into(),
        content_root: format!("skills/{id}"),
        content_hash: hash(digest),
        content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        status: ChannelSkillReleaseStatus::Updated,
    }
}

pub(super) fn target_v2() -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: 2,
        tag_name: "channel-v000002".into(),
        commit_sha: "d".repeat(40),
    }
}

fn lock_entry(name: &str, commit: &str) -> crate::lockfile::LockEntry {
    crate::lockfile::LockEntry {
        name: name.into(),
        git_url: "https://github.com/acme/channel.git".into(),
        git_ref: Some(commit.repeat(40)),
        tree_hash: "tree".into(),
        content_hash: Some(hash('a')),
        content_hash_version: Some(CHANNEL_CONTENT_HASH_VERSION),
        installed_at: "2026-08-05T00:00:00Z".into(),
        source_folder: Some(format!("skills/{name}")),
    }
}

fn repository() -> RemoteRepository {
    RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "channel".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: "https://github.com/acme/channel.git".into(),
        private: true,
        permissions: RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        },
    }
}

fn channel() -> SharedChannelDescriptor {
    SharedChannelDescriptor {
        descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        owner: "acme".into(),
        name: "channel".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: "https://github.com/acme/channel.git".into(),
        role: SharedChannelRole::Subscriber,
        status: SharedChannelStatus::Active,
        authorization: SharedChannelAuthorization::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn hash(digest: char) -> String {
    format!("sha256:{}", digest.to_string().repeat(64))
}
