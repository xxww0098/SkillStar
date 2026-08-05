use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use skillstar_skills::shared_channels::{
    CreateSharedChannelRequest, GitHubOrganization, RemoteRepository, RepositoryPermissions,
    SharedChannelError, SharedChannelErrorCode, SharedChannelFacade, SharedChannelGateway,
    SharedChannelRegistry, SharedChannelRole, SharedChannelStatus, SharedChannelStore,
    project_role,
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

#[derive(Clone, Default)]
struct FailingSaveRegistry(MemoryRegistry);

impl SharedChannelRegistry for FailingSaveRegistry {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        self.0.load()
    }

    fn save(&self, _store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::Storage,
            "disk full",
        ))
    }
}

#[derive(Clone)]
struct FakeGateway {
    state: Arc<Mutex<FakeState>>,
}

struct FakeState {
    organizations: Result<Vec<GitHubOrganization>, SharedChannelError>,
    repository: Result<RemoteRepository, SharedChannelError>,
    installation_checks: VecDeque<Result<(), SharedChannelError>>,
    access_checks: VecDeque<Result<(), SharedChannelError>>,
    create_calls: usize,
    installation_check_calls: usize,
    access_check_calls: usize,
}

impl FakeGateway {
    fn ready() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                organizations: Ok(vec![organization()]),
                repository: Ok(repository()),
                installation_checks: VecDeque::from([Ok(())]),
                access_checks: VecDeque::from([Ok(())]),
                create_calls: 0,
                installation_check_calls: 0,
                access_check_calls: 0,
            })),
        }
    }
}

#[async_trait]
impl SharedChannelGateway for FakeGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        self.state.lock().unwrap().organizations.clone()
    }

    async fn list_selected_repositories(
        &self,
        _organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        self.state
            .lock()
            .unwrap()
            .repository
            .clone()
            .map(|repository| vec![repository])
    }

    async fn create_private_repository(
        &self,
        _organization: &str,
        _name: &str,
        _description: &str,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let mut state = self.state.lock().unwrap();
        state.create_calls += 1;
        state.repository.clone()
    }

    async fn validate_selected_installation(
        &self,
        _organization_id: u64,
    ) -> Result<(), SharedChannelError> {
        let mut state = self.state.lock().unwrap();
        state.installation_check_calls += 1;
        state.installation_checks.pop_front().unwrap_or(Ok(()))
    }

    async fn get_selected_repository(
        &self,
        _organization_id: u64,
        _repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let mut state = self.state.lock().unwrap();
        state.access_check_calls += 1;
        state.access_checks.pop_front().unwrap_or(Ok(()))?;
        state.repository.clone()
    }
}

fn organization() -> GitHubOrganization {
    GitHubOrganization {
        id: 7,
        login: "acme".into(),
        avatar_url: None,
        viewer_is_admin: true,
    }
}

fn repository() -> RemoteRepository {
    RemoteRepository {
        id: 42,
        owner_id: 7,
        owner_login: "acme".into(),
        owner_type: "Organization".into(),
        name: "skillstar-team".into(),
        default_branch: "main".into(),
        html_url: "https://github.com/acme/skillstar-team".into(),
        clone_url: "https://github.com/acme/skillstar-team.git".into(),
        private: true,
        permissions: RepositoryPermissions {
            admin: true,
            maintain: true,
            push: true,
            pull: true,
        },
    }
}

fn request() -> CreateSharedChannelRequest {
    CreateSharedChannelRequest {
        organization: "acme".into(),
        repository_name: "skillstar-team".into(),
        description: "Shared SkillStar channel".into(),
    }
}

#[tokio::test]
async fn creates_private_organization_channel_and_projects_owner_role() {
    let gateway = FakeGateway::ready();
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway.clone(), registry.clone());

    let channel = facade.create_channel(request()).await.unwrap();

    assert_eq!(channel.repository_id, 42);
    assert_eq!(channel.role, SharedChannelRole::Owner);
    assert_eq!(channel.status, SharedChannelStatus::Active);
    assert_eq!(channel.descriptor_version, 1);
    assert_eq!(registry.load().unwrap().schema_version, 1);
    assert_eq!(gateway.state.lock().unwrap().create_calls, 1);
}

#[tokio::test]
async fn reports_when_the_identity_has_no_organizations() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().organizations = Ok(Vec::new());
    let facade = SharedChannelFacade::new(gateway, MemoryRegistry::default());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::NoOrganizations);
}

#[tokio::test]
async fn preserves_permission_denied_as_an_actionable_error() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().repository = Err(SharedChannelError::new(
        SharedChannelErrorCode::PermissionDenied,
        "Administration write is required",
    ));
    let facade = SharedChannelFacade::new(gateway, MemoryRegistry::default());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
}

#[tokio::test]
async fn missing_app_permissions_stop_before_repository_creation() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().installation_checks =
        VecDeque::from([Err(SharedChannelError::new(
            SharedChannelErrorCode::PermissionDenied,
            "Administration write and Contents write are required",
        ))]);
    let facade = SharedChannelFacade::new(gateway.clone(), MemoryRegistry::default());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
    assert_eq!(gateway.state.lock().unwrap().create_calls, 0);
}

#[tokio::test]
async fn app_not_installed_stops_before_repository_creation() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().installation_checks =
        VecDeque::from([Err(SharedChannelError::new(
            SharedChannelErrorCode::AppNotInstalled,
            "Install the SkillStar GitHub App for acme",
        ))]);
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway.clone(), registry.clone());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::AppNotInstalled);
    assert!(registry.load().unwrap().channels.is_empty());
    assert_eq!(gateway.state.lock().unwrap().create_calls, 0);
}

#[tokio::test]
async fn registry_write_failure_reports_the_created_repository_for_manual_recovery() {
    let gateway = FakeGateway::ready();
    let facade = SharedChannelFacade::new(gateway.clone(), FailingSaveRegistry::default());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(error.message.contains("acme/skillstar-team"));
    assert!(error.message.contains("repository ID 42"));
    assert_eq!(gateway.state.lock().unwrap().create_calls, 1);
}

#[tokio::test]
async fn rejects_personal_and_public_repositories_before_binding() {
    for (mut remote, expected) in [
        (
            {
                let mut remote = repository();
                remote.owner_type = "User".into();
                remote
            },
            SharedChannelErrorCode::PersonalOwnerRejected,
        ),
        (
            {
                let mut remote = repository();
                remote.private = false;
                remote
            },
            SharedChannelErrorCode::PublicRepositoryRejected,
        ),
    ] {
        remote.permissions.admin = true;
        let gateway = FakeGateway::ready();
        gateway.state.lock().unwrap().repository = Ok(remote);
        let facade = SharedChannelFacade::new(gateway, MemoryRegistry::default());
        let error = facade.create_channel(request()).await.unwrap_err();
        assert_eq!(error.code, expected);
    }
}

#[tokio::test]
async fn rejects_non_github_repository_routes() {
    let gateway = FakeGateway::ready();
    let mut remote = repository();
    remote.clone_url = "https://git.example.com/acme/skillstar-team.git".into();
    gateway.state.lock().unwrap().repository = Ok(remote);
    let facade = SharedChannelFacade::new(gateway, MemoryRegistry::default());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::UnsupportedHost);
}

#[tokio::test]
async fn rejects_repository_urls_with_mismatched_paths_or_query_data() {
    for html_url in [
        "https://github.com/acme/another-repository",
        "https://github.com/acme/skillstar-team?token=secret",
    ] {
        let gateway = FakeGateway::ready();
        let mut remote = repository();
        remote.html_url = html_url.into();
        gateway.state.lock().unwrap().repository = Ok(remote);
        let facade = SharedChannelFacade::new(gateway, MemoryRegistry::default());

        let error = facade.create_channel(request()).await.unwrap_err();

        assert_eq!(error.code, SharedChannelErrorCode::UnsupportedHost);
    }
}

#[test]
fn projects_repository_permissions_to_channel_roles() {
    assert_eq!(
        project_role(&RepositoryPermissions {
            admin: false,
            maintain: true,
            push: false,
            pull: true,
        }),
        Some(SharedChannelRole::Publisher)
    );
    assert_eq!(
        project_role(&RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: true,
        }),
        Some(SharedChannelRole::Subscriber)
    );
}

#[tokio::test]
async fn retries_pending_app_access_validation_without_creating_a_second_repository() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().access_checks = VecDeque::from([
        Err(SharedChannelError::new(
            SharedChannelErrorCode::AppRepositoryAccessRequired,
            "select repository",
        )),
        Ok(()),
    ]);
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway.clone(), registry);

    assert!(facade.create_channel(request()).await.is_err());
    let mut renamed = repository();
    renamed.name = "renamed-channel".into();
    renamed.html_url = "https://github.com/acme/renamed-channel".into();
    renamed.clone_url = "https://github.com/acme/renamed-channel.git".into();
    gateway.state.lock().unwrap().repository = Ok(renamed);
    let resumed = facade.resume_channel(42).await.unwrap();

    assert_eq!(resumed.status, SharedChannelStatus::Active);
    assert_eq!(resumed.repository_id, 42);
    assert_eq!(resumed.name, "renamed-channel");
    let state = gateway.state.lock().unwrap();
    assert_eq!(state.create_calls, 1);
    assert_eq!(state.access_check_calls, 2);
}

#[tokio::test]
async fn create_never_uses_mutable_owner_and_name_to_resume_pending_identity() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().access_checks = VecDeque::from([Err(SharedChannelError::new(
        SharedChannelErrorCode::AppRepositoryAccessRequired,
        "select repository",
    ))]);
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway.clone(), registry);

    assert!(facade.create_channel(request()).await.is_err());
    gateway.state.lock().unwrap().repository = Err(SharedChannelError::new(
        SharedChannelErrorCode::RepositoryConflict,
        "already exists",
    ));

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryConflict);
    let state = gateway.state.lock().unwrap();
    assert_eq!(state.create_calls, 2);
    assert_eq!(state.access_check_calls, 1);
}

#[tokio::test]
async fn resume_rejects_a_repository_response_with_a_different_numeric_id() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().access_checks = VecDeque::from([Err(SharedChannelError::new(
        SharedChannelErrorCode::AppRepositoryAccessRequired,
        "select repository",
    ))]);
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway.clone(), registry);
    assert!(facade.create_channel(request()).await.is_err());

    let mut wrong_repository = repository();
    wrong_repository.id = 43;
    gateway.state.lock().unwrap().repository = Ok(wrong_repository);

    let error = facade.resume_channel(42).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryNotFound);
}

#[tokio::test]
async fn concurrent_facades_preserve_both_registry_updates() {
    let first_gateway = FakeGateway::ready();
    let second_gateway = FakeGateway::ready();
    let mut second_repository = repository();
    second_repository.id = 43;
    second_repository.name = "second-channel".into();
    second_repository.html_url = "https://github.com/acme/second-channel".into();
    second_repository.clone_url = "https://github.com/acme/second-channel.git".into();
    second_gateway.state.lock().unwrap().repository = Ok(second_repository);
    let registry = MemoryRegistry::default();
    let first = SharedChannelFacade::new(first_gateway, registry.clone());
    let second = SharedChannelFacade::new(second_gateway, registry.clone());
    let mut second_request = request();
    second_request.repository_name = "second-channel".into();

    let (first_result, second_result) = tokio::join!(
        first.create_channel(request()),
        second.create_channel(second_request)
    );

    first_result.unwrap();
    second_result.unwrap();
    let mut ids = registry
        .load()
        .unwrap()
        .channels
        .into_iter()
        .map(|channel| channel.repository_id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, vec![42, 43]);
}
