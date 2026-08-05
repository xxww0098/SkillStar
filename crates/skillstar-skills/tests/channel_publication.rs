use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use skillstar_skills::shared_channels::{
    ChannelDraftScanner, ChannelDraftSkill, ChannelDraftSnapshot, ChannelPublicationFacade,
    ChannelPublicationGateway, ChannelPublishSessions, ChannelPublisherIdentity,
    ChannelReleaseManifest, ChannelReleaseSkill, ChannelSkillReleaseStatus, GitHubOrganization,
    RemoteChannelRelease, RemoteRepository, RepositoryPermissions, SharedChannelAuthorization,
    SharedChannelDescriptor, SharedChannelError, SharedChannelErrorCode, SharedChannelGateway,
    SharedChannelRegistry, SharedChannelRole, SharedChannelStatus, SharedChannelStore,
    validate_manifest,
};

#[derive(Clone, Default)]
struct MemoryRegistry(Arc<Mutex<SharedChannelStore>>);

impl SharedChannelRegistry for MemoryRegistry {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        *self.0.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[derive(Clone)]
struct FakeGateway(Arc<Mutex<FakeGatewayState>>);

#[derive(Clone)]
struct FakeGatewayState {
    repository: RemoteRepository,
    head: String,
    publisher: ChannelPublisherIdentity,
    manifests: Vec<ChannelReleaseManifest>,
    highest_reserved_revision: u64,
    publish_result: Result<RemoteChannelRelease, SharedChannelError>,
    published: Vec<ChannelReleaseManifest>,
}

impl FakeGateway {
    fn ready() -> Self {
        Self(Arc::new(Mutex::new(FakeGatewayState {
            repository: repository(permissions(true, true, true)),
            head: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            publisher: ChannelPublisherIdentity {
                id: 99,
                login: "alice".into(),
            },
            manifests: Vec::new(),
            highest_reserved_revision: 0,
            publish_result: Ok(RemoteChannelRelease {
                id: 501,
                html_url: "https://github.com/acme/shared/releases/tag/channel-v000001".into(),
            }),
            published: Vec::new(),
        })))
    }
}

#[async_trait]
impl SharedChannelGateway for FakeGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        Ok(vec![organization()])
    }

    async fn list_selected_repositories(
        &self,
        _organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        Ok(vec![self.0.lock().unwrap().repository.clone()])
    }

    async fn create_private_repository(
        &self,
        _organization: &str,
        _name: &str,
        _description: &str,
    ) -> Result<RemoteRepository, SharedChannelError> {
        Ok(self.0.lock().unwrap().repository.clone())
    }

    async fn validate_selected_installation(
        &self,
        _organization_id: u64,
    ) -> Result<(), SharedChannelError> {
        Ok(())
    }

    async fn get_selected_repository(
        &self,
        _organization_id: u64,
        _repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        Ok(self.0.lock().unwrap().repository.clone())
    }
}

#[async_trait]
impl ChannelPublicationGateway for FakeGateway {
    async fn publisher_identity(&self) -> Result<ChannelPublisherIdentity, SharedChannelError> {
        Ok(self.0.lock().unwrap().publisher.clone())
    }

    async fn head_commit(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<String, SharedChannelError> {
        Ok(self.0.lock().unwrap().head.clone())
    }

    async fn published_manifests(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<Vec<ChannelReleaseManifest>, SharedChannelError> {
        Ok(self.0.lock().unwrap().manifests.clone())
    }

    async fn highest_reserved_revision(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<u64, SharedChannelError> {
        Ok(self.0.lock().unwrap().highest_reserved_revision)
    }

    async fn publish_immutable(
        &self,
        _repository: &RemoteRepository,
        manifest: &ChannelReleaseManifest,
    ) -> Result<RemoteChannelRelease, SharedChannelError> {
        let mut state = self.0.lock().unwrap();
        let result = state.publish_result.clone()?;
        state.published.push(manifest.clone());
        Ok(result)
    }
}

#[derive(Clone)]
struct FakeScanner {
    snapshot: Arc<Mutex<Result<ChannelDraftSnapshot, SharedChannelError>>>,
    calls: Arc<Mutex<usize>>,
}

impl FakeScanner {
    fn ready(skills: Vec<ChannelDraftSkill>) -> Self {
        Self {
            snapshot: Arc::new(Mutex::new(Ok(ChannelDraftSnapshot {
                commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                skills,
            }))),
            calls: Arc::new(Mutex::new(0)),
        }
    }
}

impl ChannelDraftScanner for FakeScanner {
    fn scan(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<ChannelDraftSnapshot, SharedChannelError> {
        *self.calls.lock().unwrap() += 1;
        self.snapshot.lock().unwrap().clone()
    }
}

fn facade(
    gateway: FakeGateway,
    scanner: FakeScanner,
) -> ChannelPublicationFacade<FakeGateway, MemoryRegistry> {
    ChannelPublicationFacade::new(
        gateway,
        registry(),
        scanner,
        ChannelPublishSessions::default(),
    )
}

fn registry() -> MemoryRegistry {
    let registry = MemoryRegistry::default();
    registry.0.lock().unwrap().channels.push(channel());
    registry
}

fn session_id() -> String {
    "123e4567-e89b-12d3-a456-426614174000".into()
}

fn organization() -> GitHubOrganization {
    GitHubOrganization {
        id: 7,
        login: "acme".into(),
        avatar_url: None,
        viewer_is_admin: true,
    }
}

fn permissions(admin: bool, maintain: bool, push: bool) -> RepositoryPermissions {
    RepositoryPermissions {
        admin,
        maintain,
        push,
        pull: true,
    }
}

fn repository(permissions: RepositoryPermissions) -> RemoteRepository {
    RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "shared".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/shared".into(),
        clone_url: "https://github.com/acme/shared.git".into(),
        private: true,
        permissions,
    }
}

fn channel() -> SharedChannelDescriptor {
    SharedChannelDescriptor {
        descriptor_version: 1,
        repository_id: 42,
        organization_id: 7,
        owner: "acme".into(),
        name: "shared".into(),
        html_url: "https://github.com/acme/shared".into(),
        clone_url: "https://github.com/acme/shared.git".into(),
        role: SharedChannelRole::Owner,
        status: SharedChannelStatus::Active,
        authorization: SharedChannelAuthorization::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn draft(id: &str, root: &str, hash: &str) -> ChannelDraftSkill {
    ChannelDraftSkill {
        id: id.into(),
        content_root: root.into(),
        content_hash: fixture_hash(hash),
    }
}

fn prior_manifest(skills: Vec<ChannelReleaseSkill>) -> ChannelReleaseManifest {
    ChannelReleaseManifest {
        schema_version: 1,
        repository_id: 42,
        organization_id: 7,
        revision: 1,
        tag_name: "channel-v000001".into(),
        commit_sha: "1111111111111111111111111111111111111111".into(),
        publisher: ChannelPublisherIdentity {
            id: 77,
            login: "bob".into(),
        },
        published_at: "2026-08-04T00:00:00Z".into(),
        title: "Initial".into(),
        notes: "First".into(),
        skills,
    }
}

fn released_skill(
    id: &str,
    root: &str,
    hash: &str,
    status: ChannelSkillReleaseStatus,
) -> ChannelReleaseSkill {
    ChannelReleaseSkill {
        id: id.into(),
        content_root: root.into(),
        content_hash: fixture_hash(hash),
        content_hash_version: 2,
        status,
    }
}

fn fixture_hash(label: &str) -> String {
    let digest = Sha256::digest(label.as_bytes());
    let mut value = String::from("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

#[tokio::test]
async fn first_publish_creates_revision_one_bound_to_the_previewed_commit() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("writer", "skills/writer", "sha256:writer")]),
    );

    let preview = facade.preview(42, session_id()).await.unwrap();
    assert_eq!(preview.next_revision, 1);
    assert_eq!(preview.tag_name, "channel-v000001");
    assert_eq!(
        preview.commit_sha,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(preview.changes[0].status, ChannelSkillReleaseStatus::Added);

    let result = facade
        .publish(
            &session_id(),
            "Writing tools".into(),
            "Initial release".into(),
        )
        .await
        .unwrap();

    assert_eq!(result.manifest.revision, 1);
    assert_eq!(result.manifest.publisher.login, "alice");
    assert_eq!(result.manifest.title, "Writing tools");
    assert_eq!(result.manifest.notes, "Initial release");
    assert_eq!(result.manifest.commit_sha, preview.commit_sha);
    assert_eq!(gateway.0.lock().unwrap().published.len(), 1);
}

#[tokio::test]
async fn next_preview_detects_additions_updates_removals_and_unchanged_skills() {
    let gateway = FakeGateway::ready();
    gateway.0.lock().unwrap().manifests = vec![prior_manifest(vec![
        released_skill(
            "same",
            "skills/same",
            "sha256:same",
            ChannelSkillReleaseStatus::Added,
        ),
        released_skill(
            "changed",
            "skills/changed",
            "sha256:old",
            ChannelSkillReleaseStatus::Added,
        ),
        released_skill(
            "removed",
            "skills/removed",
            "sha256:removed",
            ChannelSkillReleaseStatus::Added,
        ),
    ])];
    let facade = facade(
        gateway,
        FakeScanner::ready(vec![
            draft("added", "skills/added", "sha256:new"),
            draft("changed", "skills/changed", "sha256:new"),
            draft("same", "skills/same", "sha256:same"),
        ]),
    );

    let preview = facade.preview(42, session_id()).await.unwrap();

    assert_eq!(preview.next_revision, 2);
    assert_eq!(preview.tag_name, "channel-v000002");
    let statuses = preview
        .changes
        .iter()
        .map(|skill| (skill.id.as_str(), skill.status))
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            ("added", ChannelSkillReleaseStatus::Added),
            ("changed", ChannelSkillReleaseStatus::Updated),
            ("removed", ChannelSkillReleaseStatus::Removed),
            ("same", ChannelSkillReleaseStatus::Unchanged),
        ]
    );
}

#[tokio::test]
async fn read_only_users_are_rejected_before_scanning() {
    let gateway = FakeGateway::ready();
    gateway.0.lock().unwrap().repository.permissions = permissions(false, false, false);
    let scanner = FakeScanner::ready(vec![]);
    let facade = facade(gateway, scanner.clone());

    let error = facade.preview(42, session_id()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(*scanner.calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn maintain_or_write_publishers_can_publish_without_admin() {
    for permissions in [
        permissions(false, true, false),
        permissions(false, false, true),
    ] {
        let gateway = FakeGateway::ready();
        gateway.0.lock().unwrap().repository.permissions = permissions;
        gateway.0.lock().unwrap().publisher = ChannelPublisherIdentity {
            id: 88,
            login: "publisher".into(),
        };
        let facade = facade(
            gateway,
            FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
        );

        facade.preview(42, session_id()).await.unwrap();
        let result = facade
            .publish(&session_id(), "Release".into(), String::new())
            .await
            .unwrap();
        assert_eq!(result.manifest.publisher.login, "publisher");
    }
}

#[tokio::test]
async fn ordinary_draft_preview_does_not_create_a_visible_release() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );

    facade.preview(42, session_id()).await.unwrap();

    assert!(gateway.0.lock().unwrap().published.is_empty());
}

#[tokio::test]
async fn orphaned_tag_reserves_a_revision_without_becoming_a_published_manifest() {
    let gateway = FakeGateway::ready();
    gateway.0.lock().unwrap().highest_reserved_revision = 3;
    let facade = facade(
        gateway,
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );

    let preview = facade.preview(42, session_id()).await.unwrap();

    assert_eq!(preview.next_revision, 4);
    assert_eq!(preview.tag_name, "channel-v000004");
    assert!(
        preview
            .changes
            .iter()
            .all(|skill| { skill.status == ChannelSkillReleaseStatus::Added })
    );
}

#[tokio::test]
async fn another_publishers_reserved_revision_invalidates_the_preview() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );
    facade.preview(42, session_id()).await.unwrap();
    gateway.0.lock().unwrap().highest_reserved_revision = 1;

    let error = facade
        .publish(&session_id(), "Release".into(), String::new())
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::ReleaseConflict);
    assert!(gateway.0.lock().unwrap().published.is_empty());
}

#[tokio::test]
async fn changed_head_blocks_publish_and_keeps_the_preview_for_rescan_or_retry() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );
    facade.preview(42, session_id()).await.unwrap();
    gateway.0.lock().unwrap().head = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into();

    let error = facade
        .publish(&session_id(), "Release".into(), String::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::DraftChanged);
    gateway.0.lock().unwrap().head = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    assert!(
        facade
            .publish(&session_id(), "Release".into(), String::new())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn repository_privacy_and_owner_contract_is_revalidated_before_publish() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );
    facade.preview(42, session_id()).await.unwrap();
    gateway.0.lock().unwrap().repository.private = false;

    let error = facade
        .publish(&session_id(), "Release".into(), String::new())
        .await
        .unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PublicRepositoryRejected);
    assert!(gateway.0.lock().unwrap().published.is_empty());
}

#[tokio::test]
async fn workflow_permission_rejection_is_precise_and_retryable() {
    let gateway = FakeGateway::ready();
    gateway.0.lock().unwrap().publish_result = Err(SharedChannelError::new(
        SharedChannelErrorCode::WorkflowPermissionRequired,
        "GitHub refused the release because workflow authorization is not granted",
    ));
    let facade = facade(
        gateway.clone(),
        FakeScanner::ready(vec![draft("one", "skills/one", "sha256:one")]),
    );
    facade.preview(42, session_id()).await.unwrap();

    let error = facade
        .publish(&session_id(), "Release".into(), String::new())
        .await
        .unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::WorkflowPermissionRequired
    );
    gateway.0.lock().unwrap().publish_result = Ok(RemoteChannelRelease {
        id: 502,
        html_url: "https://github.com/acme/shared/releases/tag/channel-v000001".into(),
    });
    assert!(
        facade
            .publish(&session_id(), "Release".into(), String::new())
            .await
            .is_ok()
    );
}

#[test]
fn release_manifest_round_trips_without_credentials() {
    let manifest = prior_manifest(vec![released_skill(
        "writer",
        "skills/writer",
        "sha256:known-independent-fixture",
        ChannelSkillReleaseStatus::Added,
    )]);

    let json = serde_json::to_string(&manifest).unwrap();
    let decoded: ChannelReleaseManifest = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, manifest);
    assert!(!json.to_ascii_lowercase().contains("token"));
    assert!(!json.contains("https://x-access-token:"));
}

#[test]
fn release_manifest_rejects_case_insensitive_duplicate_skill_identities() {
    let manifest = prior_manifest(vec![
        released_skill(
            "Writer",
            "skills/one",
            "sha256:one",
            ChannelSkillReleaseStatus::Added,
        ),
        released_skill(
            "writer",
            "skills/two",
            "sha256:two",
            ChannelSkillReleaseStatus::Added,
        ),
    ]);

    let error = validate_manifest(&manifest, 42, 7).unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
}

#[test]
fn release_manifest_rejects_skill_identities_that_cannot_be_materialized_safely() {
    let manifest = prior_manifest(vec![released_skill(
        "CON",
        "skills/con",
        "sha256:con",
        ChannelSkillReleaseStatus::Added,
    )]);

    let error = validate_manifest(&manifest, 42, 7).unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
}

#[test]
fn release_manifest_schema_rejects_unknown_fields() {
    let manifest = prior_manifest(Vec::new());
    let mut value = serde_json::to_value(manifest).unwrap();
    value["future_behavior"] = serde_json::json!(true);

    assert!(serde_json::from_value::<ChannelReleaseManifest>(value).is_err());
}

#[test]
fn release_manifest_rejects_control_characters_in_content_roots() {
    let manifest = prior_manifest(vec![released_skill(
        "writer",
        "skills/\0writer",
        "sha256:writer",
        ChannelSkillReleaseStatus::Added,
    )]);

    let error = validate_manifest(&manifest, 42, 7).unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Integrity);
}
