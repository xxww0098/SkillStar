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

#[derive(Clone)]
struct FakeGateway {
    state: Arc<Mutex<FakeState>>,
}

struct FakeState {
    organizations: Result<Vec<GitHubOrganization>, SharedChannelError>,
    repository: Result<RemoteRepository, SharedChannelError>,
    authorizations: VecDeque<Result<(), SharedChannelError>>,
    create_calls: usize,
    authorize_calls: usize,
}

impl FakeGateway {
    fn ready() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                organizations: Ok(vec![organization()]),
                repository: Ok(repository()),
                authorizations: VecDeque::from([Ok(())]),
                create_calls: 0,
                authorize_calls: 0,
            })),
        }
    }
}

#[async_trait]
impl SharedChannelGateway for FakeGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        self.state.lock().unwrap().organizations.clone()
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

    async fn get_repository(
        &self,
        _repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        self.state.lock().unwrap().repository.clone()
    }

    async fn authorize_selected_repository(
        &self,
        _organization_id: u64,
        _repository_id: u64,
    ) -> Result<(), SharedChannelError> {
        let mut state = self.state.lock().unwrap();
        state.authorize_calls += 1;
        state.authorizations.pop_front().unwrap_or(Ok(()))
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
async fn app_not_installed_leaves_a_resumable_pending_channel() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().authorizations = VecDeque::from([Err(SharedChannelError::new(
        SharedChannelErrorCode::AppNotInstalled,
        "Install the SkillStar GitHub App for acme",
    ))]);
    let registry = MemoryRegistry::default();
    let facade = SharedChannelFacade::new(gateway, registry.clone());

    let error = facade.create_channel(request()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::AppNotInstalled);
    let pending = &registry.load().unwrap().channels[0];
    assert_eq!(pending.repository_id, 42);
    assert_eq!(pending.status, SharedChannelStatus::AwaitingAppInstallation);
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
async fn retries_pending_authorization_without_creating_a_second_repository() {
    let gateway = FakeGateway::ready();
    gateway.state.lock().unwrap().authorizations = VecDeque::from([
        Err(SharedChannelError::new(
            SharedChannelErrorCode::AppNotInstalled,
            "install app",
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
    assert_eq!(state.authorize_calls, 2);
}
