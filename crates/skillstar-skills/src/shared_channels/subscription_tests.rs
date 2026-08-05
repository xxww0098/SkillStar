use super::*;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeGateway {
    repository: RemoteRepository,
    manifests: Arc<Mutex<Vec<ChannelReleaseManifest>>>,
}

#[async_trait]
impl ChannelSubscriptionGateway for FakeGateway {
    async fn accessible_repository(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        if repository_id == self.repository.id {
            Ok(self.repository.clone())
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
struct FakeChannels(Arc<Mutex<SharedChannelStore>>);

#[async_trait]
impl SharedChannelRegistry for FakeChannels {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        *self.0.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FakeSubscriptions {
    store: Arc<Mutex<ChannelSubscriptionStore>>,
    read_only: Arc<Mutex<Option<Vec<ChannelSubscriptionView>>>>,
    fail_save: Arc<Mutex<bool>>,
}

impl ChannelSubscriptionRegistry for FakeSubscriptions {
    fn list_views(&self) -> Result<Vec<ChannelSubscriptionView>, SharedChannelError> {
        if let Some(views) = self.read_only.lock().unwrap().clone() {
            return Ok(views);
        }
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
        if self.read_only.lock().unwrap().is_some() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionSchemaUnsupported,
                "read only",
            ));
        }
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

#[derive(Clone)]
struct FakeInstaller {
    requests: Arc<Mutex<Vec<ChannelInstallRequest>>>,
    rollbacks: Arc<Mutex<Vec<ChannelInstallReceipt>>>,
    fail: Arc<Mutex<bool>>,
    invalid_receipt: Arc<Mutex<bool>>,
    fail_rollback: Arc<Mutex<bool>>,
}

impl Default for FakeInstaller {
    fn default() -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            rollbacks: Arc::new(Mutex::new(Vec::new())),
            fail: Arc::new(Mutex::new(false)),
            invalid_receipt: Arc::new(Mutex::new(false)),
            fail_rollback: Arc::new(Mutex::new(false)),
        }
    }
}

#[async_trait]
impl ChannelSubscriptionInstaller for FakeInstaller {
    async fn install(
        &self,
        request: ChannelInstallRequest,
    ) -> Result<ChannelInstallReceipt, SharedChannelError> {
        self.requests.lock().unwrap().push(request.clone());
        if *self.fail.lock().unwrap() {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionInstallFailed,
                "install failed",
            ));
        }
        let mut skills = request
            .manifest
            .skills
            .iter()
            .filter(|skill| request.selected_skill_ids.contains(&skill.id))
            .map(|skill| installed_skill(&request, skill))
            .collect::<Vec<_>>();
        if *self.invalid_receipt.lock().unwrap()
            && let Some(skill) = skills.first_mut()
        {
            skill.baseline_hash = hash('f');
        }
        Ok(ChannelInstallReceipt {
            newly_installed_skill_ids: skills.iter().map(|skill| skill.id.clone()).collect(),
            skills,
        })
    }

    async fn rollback(&self, receipt: &ChannelInstallReceipt) -> Result<(), SharedChannelError> {
        self.rollbacks.lock().unwrap().push(receipt.clone());
        if *self.fail_rollback.lock().unwrap() {
            Err(SharedChannelError::new(
                SharedChannelErrorCode::SubscriptionInstallFailed,
                "rollback failed",
            ))
        } else {
            Ok(())
        }
    }
}

fn facade(
    subscriptions: FakeSubscriptions,
    installer: FakeInstaller,
) -> ChannelSubscriptionFacade<FakeGateway, FakeChannels, FakeSubscriptions, FakeInstaller> {
    facade_with_repository(subscriptions, installer, repository())
}

fn facade_with_repository(
    subscriptions: FakeSubscriptions,
    installer: FakeInstaller,
    repository: RemoteRepository,
) -> ChannelSubscriptionFacade<FakeGateway, FakeChannels, FakeSubscriptions, FakeInstaller> {
    ChannelSubscriptionFacade::new(
        FakeGateway {
            repository,
            manifests: Arc::new(Mutex::new(vec![manifest()])),
        },
        FakeChannels(Arc::new(Mutex::new(SharedChannelStore {
            schema_version: SHARED_CHANNEL_STORE_VERSION,
            channels: vec![channel()],
        }))),
        subscriptions,
        installer,
    )
}

#[tokio::test]
async fn first_review_defaults_every_current_skill_to_selected() {
    let review = facade(FakeSubscriptions::default(), FakeInstaller::default())
        .review(42)
        .await
        .unwrap();

    assert_eq!(review.target.revision, 1);
    assert_eq!(review.title, "First release");
    assert_eq!(review.notes, "Install the useful Skills");
    assert!(review.exposure.full_repository_contents_readable);
    assert!(review.exposure.full_history_readable);
    assert_eq!(
        review
            .skills
            .iter()
            .map(|skill| (&skill.id, skill.selected))
            .collect::<Vec<_>>(),
        vec![(&"reader".to_string(), true), (&"writer".to_string(), true)]
    );
}

#[tokio::test]
async fn subscribe_installs_only_selected_skills_and_persists_target_baseline_and_provenance() {
    let subscriptions = FakeSubscriptions::default();
    let installer = FakeInstaller::default();
    let service = facade(subscriptions.clone(), installer.clone());

    let result = service
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: vec!["writer".into()],
        })
        .await
        .unwrap();

    assert_eq!(result.target.tag_name, "channel-v000001");
    assert_eq!(result.skills.len(), 1);
    assert_eq!(result.skills[0].id, "writer");
    assert_eq!(result.skills[0].baseline_hash, hash('c'));
    assert_eq!(result.skills[0].provenance.repository_id, 42);
    assert_eq!(result.skills[0].provenance.git_ref, "a".repeat(40));
    assert_eq!(
        installer.requests.lock().unwrap()[0].selected_skill_ids,
        vec!["writer"]
    );

    let restarted = facade(subscriptions, FakeInstaller::default());
    let review = restarted.review(42).await.unwrap();
    assert_eq!(
        review
            .skills
            .iter()
            .map(|skill| (&skill.id, skill.selected))
            .collect::<Vec<_>>(),
        vec![
            (&"reader".to_string(), false),
            (&"writer".to_string(), true)
        ]
    );
}

#[tokio::test]
async fn idempotent_subscribe_clears_a_stale_revoked_state_after_remote_validation() {
    let subscriptions = FakeSubscriptions::default();
    let installer = FakeInstaller::default();
    let service = facade(subscriptions.clone(), installer.clone());
    let request = SubscribeChannelRequest {
        repository_id: 42,
        target: release_target(),
        selected_skill_ids: vec!["writer".into()],
    };
    service.subscribe(request.clone()).await.unwrap();
    subscriptions.store.lock().unwrap().subscriptions[0].remote_state =
        ChannelSubscriptionRemoteState::revoked("stale");

    let result = service.subscribe(request).await.unwrap();

    assert_eq!(
        result.remote_state.status,
        ChannelSubscriptionRemoteStatus::Active
    );
    assert_eq!(installer.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn empty_selection_is_a_valid_persisted_subscription() {
    let subscriptions = FakeSubscriptions::default();
    let installer = FakeInstaller::default();
    let result = facade(subscriptions.clone(), installer.clone())
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: Vec::new(),
        })
        .await
        .unwrap();

    assert!(result.skills.is_empty());
    assert!(
        installer.requests.lock().unwrap()[0]
            .selected_skill_ids
            .is_empty()
    );
    assert!(
        subscriptions.store.lock().unwrap().subscriptions[0]
            .skills
            .is_empty()
    );
}

#[tokio::test]
async fn install_failure_keeps_subscription_store_unchanged() {
    let subscriptions = FakeSubscriptions::default();
    let installer = FakeInstaller::default();
    *installer.fail.lock().unwrap() = true;

    let error = facade(subscriptions.clone(), installer)
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: vec!["writer".into()],
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionInstallFailed
    );
    assert!(subscriptions.store.lock().unwrap().subscriptions.is_empty());
}

#[tokio::test]
async fn persistence_failure_rolls_back_every_new_install() {
    let subscriptions = FakeSubscriptions::default();
    *subscriptions.fail_save.lock().unwrap() = true;
    let installer = FakeInstaller::default();

    let error = facade(subscriptions.clone(), installer.clone())
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: vec!["reader".into(), "writer".into()],
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(subscriptions.store.lock().unwrap().subscriptions.is_empty());
    assert_eq!(
        installer.rollbacks.lock().unwrap()[0].newly_installed_skill_ids,
        vec!["reader", "writer"]
    );
}

#[tokio::test]
async fn invalid_receipt_with_failed_rollback_reports_incomplete_cleanup() {
    let subscriptions = FakeSubscriptions::default();
    let installer = FakeInstaller::default();
    *installer.invalid_receipt.lock().unwrap() = true;
    *installer.fail_rollback.lock().unwrap() = true;

    let error = facade(subscriptions.clone(), installer.clone())
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: vec!["writer".into()],
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionInstallFailed
    );
    assert!(error.message.contains("rollback is incomplete"));
    assert!(subscriptions.store.lock().unwrap().subscriptions.is_empty());
    assert_eq!(installer.rollbacks.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn unknown_store_schema_is_reviewable_but_rejects_subscription_changes() {
    let subscriptions = FakeSubscriptions::default();
    *subscriptions.read_only.lock().unwrap() = Some(vec![ChannelSubscriptionView {
        schema_version: 99,
        descriptor_version: 5,
        repository_id: 42,
        organization_id: Some(7),
        target: Some(ChannelReleaseTarget {
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: "a".repeat(40),
        }),
        selected_skill_ids: vec!["writer".into()],
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        read_only: true,
    }]);
    let installer = FakeInstaller::default();
    let service = facade(subscriptions, installer.clone());

    let review = service.review(42).await.unwrap();
    assert!(review.read_only);
    assert!(!review.skills[0].selected);
    assert!(review.skills[1].selected);
    let error = service
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: release_target(),
            selected_skill_ids: vec!["writer".into()],
        })
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        SharedChannelErrorCode::SubscriptionSchemaUnsupported
    );
    assert!(installer.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn remote_access_errors_do_not_get_hidden_by_a_read_only_future_schema() {
    let subscriptions = FakeSubscriptions::default();
    *subscriptions.read_only.lock().unwrap() = Some(vec![ChannelSubscriptionView {
        schema_version: 99,
        descriptor_version: 5,
        repository_id: 42,
        organization_id: Some(7),
        target: Some(release_target()),
        selected_skill_ids: vec!["writer".into()],
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        read_only: true,
    }]);
    let mut inaccessible = repository();
    inaccessible.permissions = RepositoryPermissions {
        admin: false,
        maintain: false,
        push: false,
        pull: false,
    };

    let error = facade_with_repository(subscriptions, FakeInstaller::default(), inaccessible)
        .review(42)
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::AppRepositoryAccessRequired
    );
}

#[tokio::test]
async fn stale_review_revision_is_rejected_before_install() {
    let installer = FakeInstaller::default();
    let error = facade(FakeSubscriptions::default(), installer.clone())
        .subscribe(SubscribeChannelRequest {
            repository_id: 42,
            target: ChannelReleaseTarget {
                revision: 1,
                tag_name: "channel-v000001".into(),
                commit_sha: "f".repeat(40),
            },
            selected_skill_ids: vec!["writer".into()],
        })
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
    assert!(installer.requests.lock().unwrap().is_empty());
}

#[test]
fn serialized_subscription_provenance_never_contains_credentials() {
    let request = ChannelInstallRequest {
        repository: repository(),
        manifest: manifest(),
        selected_skill_ids: vec!["writer".into()],
    };
    let subscription = ChannelSubscription {
        descriptor_version: CHANNEL_SUBSCRIPTION_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        target: ChannelReleaseTarget {
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: "a".repeat(40),
        },
        skills: vec![installed_skill(&request, &manifest().skills[1])],
        known_skill_ids: vec!["writer".into()],
        pins: Vec::new(),
        last_update: None,
        auto_update: ChannelAutoUpdateState::default(),
        remote_state: ChannelSubscriptionRemoteState::default(),
        created_at: "now".into(),
        updated_at: "now".into(),
    };
    let json = serde_json::to_string(&subscription).unwrap();
    assert!(json.contains("https://github.com/acme/channel.git"));
    assert!(!json.contains("secret-token"));
    assert!(!json.contains("authorization"));
}

fn installed_skill(
    request: &ChannelInstallRequest,
    skill: &ChannelReleaseSkill,
) -> ChannelSubscribedSkill {
    ChannelSubscribedSkill {
        id: skill.id.clone(),
        content_root: skill.content_root.clone(),
        release_content_hash: skill.content_hash.clone(),
        release_content_hash_version: skill.content_hash_version,
        baseline_hash: skill.content_hash.clone(),
        baseline_hash_version: CHANNEL_CONTENT_HASH_VERSION,
        provenance: ChannelSkillProvenance {
            repository_id: request.repository.id,
            repository_url: request.repository.clone_url.clone(),
            git_ref: request.manifest.commit_sha.clone(),
            source_folder: skill.content_root.clone(),
        },
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

fn manifest() -> ChannelReleaseManifest {
    ChannelReleaseManifest {
        schema_version: CHANNEL_RELEASE_MANIFEST_VERSION,
        repository_id: 42,
        organization_id: 7,
        revision: 1,
        tag_name: "channel-v000001".into(),
        commit_sha: "a".repeat(40),
        publisher: ChannelPublisherIdentity {
            id: 9,
            login: "alice".into(),
        },
        published_at: "2026-08-05T00:00:00Z".into(),
        title: "First release".into(),
        notes: "Install the useful Skills".into(),
        skills: vec![
            ChannelReleaseSkill {
                id: "reader".into(),
                content_root: "skills/reader".into(),
                content_hash: hash('b'),
                content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
                status: ChannelSkillReleaseStatus::Added,
            },
            ChannelReleaseSkill {
                id: "writer".into(),
                content_root: "skills/writer".into(),
                content_hash: hash('c'),
                content_hash_version: CHANNEL_CONTENT_HASH_VERSION,
                status: ChannelSkillReleaseStatus::Added,
            },
        ],
    }
}

fn hash(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn release_target() -> ChannelReleaseTarget {
    ChannelReleaseTarget {
        revision: 1,
        tag_name: "channel-v000001".into(),
        commit_sha: "a".repeat(40),
    }
}
