use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;

use async_trait::async_trait;
use skillstar_skills::shared_channels::{
    ExistingChannelRegistrationFacade, ExistingChannelRegistrationSessions,
    ExistingChannelScanRequest, ExistingChannelSkillPreview, ExistingRepositoryInventory,
    ExistingRepositoryScanner, GitHubOrganization, RemoteRepository, RepositoryPermissions,
    SharedChannelAuthorization, SharedChannelDescriptor, SharedChannelError,
    SharedChannelErrorCode, SharedChannelGateway, SharedChannelRegistry, SharedChannelRole,
    SharedChannelStatus, SharedChannelStore,
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
struct FailOnceRegistry {
    inner: MemoryRegistry,
    fail_next_save: Arc<AtomicBool>,
}

impl SharedChannelRegistry for FailOnceRegistry {
    fn load(&self) -> Result<SharedChannelStore, SharedChannelError> {
        self.inner.load()
    }

    fn save(&self, store: &SharedChannelStore) -> Result<(), SharedChannelError> {
        if self.fail_next_save.swap(false, Ordering::SeqCst) {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                "disk unavailable",
            ));
        }
        self.inner.save(store)
    }
}

#[derive(Clone)]
struct FakeGateway(Arc<Mutex<FakeGatewayState>>);

struct FakeGatewayState {
    organizations: Result<Vec<GitHubOrganization>, SharedChannelError>,
    repository: Result<RemoteRepository, SharedChannelError>,
    candidates: Result<Vec<RemoteRepository>, SharedChannelError>,
}

impl FakeGateway {
    fn ready() -> Self {
        Self(Arc::new(Mutex::new(FakeGatewayState {
            organizations: Ok(vec![organization()]),
            repository: Ok(repository()),
            candidates: Ok(vec![repository()]),
        })))
    }
}

#[async_trait]
impl SharedChannelGateway for FakeGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        self.0.lock().unwrap().organizations.clone()
    }

    async fn list_selected_repositories(
        &self,
        _organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        self.0.lock().unwrap().candidates.clone()
    }

    async fn create_private_repository(
        &self,
        _organization: &str,
        _name: &str,
        _description: &str,
    ) -> Result<RemoteRepository, SharedChannelError> {
        self.0.lock().unwrap().repository.clone()
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
        self.0.lock().unwrap().repository.clone()
    }
}

#[derive(Clone)]
struct FakeScanner {
    result: Arc<Mutex<Result<ExistingRepositoryInventory, SharedChannelError>>>,
    calls: Arc<Mutex<usize>>,
}

struct BlockingScanner {
    started: mpsc::Sender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl ExistingRepositoryScanner for BlockingScanner {
    fn scan(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<ExistingRepositoryInventory, SharedChannelError> {
        self.started.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(inventory())
    }
}

impl FakeScanner {
    fn ready() -> Self {
        Self {
            result: Arc::new(Mutex::new(Ok(inventory()))),
            calls: Arc::new(Mutex::new(0)),
        }
    }
}

impl ExistingRepositoryScanner for FakeScanner {
    fn scan(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<ExistingRepositoryInventory, SharedChannelError> {
        *self.calls.lock().unwrap() += 1;
        self.result.lock().unwrap().clone()
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
        name: "existing-channel".into(),
        html_url: "https://github.com/acme/existing-channel".into(),
        clone_url: "https://github.com/acme/existing-channel.git".into(),
        private: true,
        permissions: RepositoryPermissions {
            admin: true,
            maintain: true,
            push: true,
            pull: true,
        },
    }
}

fn inventory() -> ExistingRepositoryInventory {
    ExistingRepositoryInventory {
        skills: vec![ExistingChannelSkillPreview {
            id: "writer".into(),
            folder_path: "skills/writer".into(),
            description: "Write clearly".into(),
        }],
        non_skill_files: vec!["README.md".into(), ".github/workflows/ci.yml".into()],
        total_files: 5,
    }
}

fn request() -> ExistingChannelScanRequest {
    ExistingChannelScanRequest {
        organization_id: 7,
        repository_id: 42,
    }
}

fn session_id() -> String {
    "123e4567-e89b-12d3-a456-426614174000".into()
}

fn registered_descriptor() -> SharedChannelDescriptor {
    SharedChannelDescriptor {
        descriptor_version: 1,
        repository_id: 42,
        organization_id: 7,
        owner: "acme".into(),
        name: "existing-channel".into(),
        html_url: "https://github.com/acme/existing-channel".into(),
        clone_url: "https://github.com/acme/existing-channel.git".into(),
        role: SharedChannelRole::Owner,
        status: SharedChannelStatus::Active,
        authorization: SharedChannelAuthorization::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn facade(
    gateway: FakeGateway,
    registry: MemoryRegistry,
    scanner: FakeScanner,
) -> ExistingChannelRegistrationFacade<FakeGateway, MemoryRegistry> {
    ExistingChannelRegistrationFacade::new(
        gateway,
        registry,
        scanner,
        ExistingChannelRegistrationSessions::default(),
    )
}

#[tokio::test]
async fn previews_full_exposure_then_confirms_an_active_channel() {
    let gateway = FakeGateway::ready();
    let registry = MemoryRegistry::default();
    let scanner = FakeScanner::ready();
    let facade = facade(gateway, registry.clone(), scanner);

    let candidates = facade.list_candidates(7).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(!candidates[0].already_registered);

    let preview = facade.scan(request(), session_id()).await.unwrap();
    assert_eq!(preview.session_id, session_id());
    assert_eq!(preview.repository.repository_id, 42);
    assert_eq!(preview.skills[0].id, "writer");
    assert_eq!(preview.non_skill_files.len(), 2);
    assert!(preview.exposure.full_repository_contents_readable);
    assert!(preview.exposure.full_history_readable);

    let channel = facade.confirm(&session_id()).await.unwrap();
    assert_eq!(channel.status, SharedChannelStatus::Active);
    assert_eq!(channel.repository_id, 42);
    assert_eq!(registry.load().unwrap().channels.len(), 1);
}

#[tokio::test]
async fn rejects_personal_public_and_non_admin_repositories_before_scanning() {
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
        (
            {
                let mut remote = repository();
                remote.permissions.admin = false;
                remote
            },
            SharedChannelErrorCode::PermissionDenied,
        ),
    ] {
        remote.permissions.pull = true;
        let gateway = FakeGateway::ready();
        gateway.0.lock().unwrap().repository = Ok(remote);
        let scanner = FakeScanner::ready();
        let facade = facade(gateway, MemoryRegistry::default(), scanner.clone());

        let error = facade.scan(request(), session_id()).await.unwrap_err();

        assert_eq!(error.code, expected);
        assert_eq!(*scanner.calls.lock().unwrap(), 0);
    }
}

#[tokio::test]
async fn reports_when_the_app_does_not_have_the_repository_selected() {
    let gateway = FakeGateway::ready();
    gateway.0.lock().unwrap().repository = Err(SharedChannelError::new(
        SharedChannelErrorCode::AppRepositoryAccessRequired,
        "select repository",
    ));
    let facade = facade(gateway, MemoryRegistry::default(), FakeScanner::ready());

    let error = facade.scan(request(), session_id()).await.unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::AppRepositoryAccessRequired
    );
}

#[tokio::test]
async fn refuses_to_bind_a_repository_id_that_is_already_registered() {
    let gateway = FakeGateway::ready();
    let registry = MemoryRegistry::default();
    registry
        .0
        .lock()
        .unwrap()
        .channels
        .push(registered_descriptor());
    let scanner = FakeScanner::ready();
    let facade = facade(gateway, registry, scanner.clone());

    let error = facade.scan(request(), session_id()).await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::RepositoryAlreadyBound);
    assert_eq!(*scanner.calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn confirm_refreshes_renamed_repository_metadata_by_stable_id() {
    let gateway = FakeGateway::ready();
    let registry = MemoryRegistry::default();
    let facade = facade(gateway.clone(), registry, FakeScanner::ready());
    facade.scan(request(), session_id()).await.unwrap();
    let mut renamed = repository();
    renamed.name = "renamed-channel".into();
    renamed.html_url = "https://github.com/acme/renamed-channel".into();
    renamed.clone_url = "https://github.com/acme/renamed-channel.git".into();
    gateway.0.lock().unwrap().repository = Ok(renamed);

    let channel = facade.confirm(&session_id()).await.unwrap();

    assert_eq!(channel.repository_id, 42);
    assert_eq!(channel.name, "renamed-channel");
}

#[tokio::test]
async fn cancelled_scan_does_not_create_a_confirmation_session() {
    let gateway = FakeGateway::ready();
    let scanner = FakeScanner::ready();
    *scanner.result.lock().unwrap() = Err(SharedChannelError::new(
        SharedChannelErrorCode::Cancelled,
        "cancelled",
    ));
    let facade = facade(gateway, MemoryRegistry::default(), scanner);

    let error = facade.scan(request(), session_id()).await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Cancelled);
    let error = facade.confirm(&session_id()).await.unwrap_err();
    assert_eq!(
        error.code,
        SharedChannelErrorCode::RegistrationSessionNotFound
    );
}

#[tokio::test]
async fn confirm_failure_keeps_the_same_session_for_retry() {
    let gateway = FakeGateway::ready();
    let facade = facade(
        gateway.clone(),
        MemoryRegistry::default(),
        FakeScanner::ready(),
    );
    facade.scan(request(), session_id()).await.unwrap();
    gateway.0.lock().unwrap().repository = Err(SharedChannelError::new(
        SharedChannelErrorCode::Network,
        "offline",
    ));

    assert!(facade.confirm(&session_id()).await.is_err());
    gateway.0.lock().unwrap().repository = Ok(repository());
    let channel = facade.confirm(&session_id()).await.unwrap();

    assert_eq!(channel.repository_id, 42);
}

#[tokio::test]
async fn registry_save_failure_keeps_the_same_session_for_retry() {
    let registry = FailOnceRegistry::default();
    registry.fail_next_save.store(true, Ordering::SeqCst);
    let facade = ExistingChannelRegistrationFacade::new(
        FakeGateway::ready(),
        registry.clone(),
        FakeScanner::ready(),
        ExistingChannelRegistrationSessions::default(),
    );
    facade.scan(request(), session_id()).await.unwrap();

    assert_eq!(
        facade.confirm(&session_id()).await.unwrap_err().code,
        SharedChannelErrorCode::Storage
    );
    let channel = facade.confirm(&session_id()).await.unwrap();

    assert_eq!(channel.repository_id, 42);
    assert_eq!(registry.load().unwrap().channels.len(), 1);
}

#[tokio::test]
async fn cancelling_a_completed_preview_discards_confirmation_state() {
    let gateway = FakeGateway::ready();
    let facade = facade(gateway, MemoryRegistry::default(), FakeScanner::ready());
    facade.scan(request(), session_id()).await.unwrap();

    assert!(facade.cancel(&session_id()));
    let error = facade.confirm(&session_id()).await.unwrap_err();

    assert_eq!(
        error.code,
        SharedChannelErrorCode::RegistrationSessionNotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_an_in_flight_scan_tombstones_its_late_result() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let sessions = ExistingChannelRegistrationSessions::default();
    let facade = Arc::new(ExistingChannelRegistrationFacade::new(
        FakeGateway::ready(),
        MemoryRegistry::default(),
        BlockingScanner {
            started: started_tx,
            release: Arc::new(Mutex::new(release_rx)),
        },
        sessions,
    ));
    let scan = tokio::spawn({
        let facade = facade.clone();
        async move { facade.scan(request(), session_id()).await }
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();

    assert!(facade.cancel(&session_id()));
    release_tx.send(()).unwrap();
    let error = scan.await.unwrap().unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Cancelled);
    assert_eq!(
        facade.confirm(&session_id()).await.unwrap_err().code,
        SharedChannelErrorCode::RegistrationSessionNotFound
    );
}

#[tokio::test]
async fn clearing_auth_scoped_sessions_invalidates_existing_previews() {
    let sessions = ExistingChannelRegistrationSessions::default();
    let facade = ExistingChannelRegistrationFacade::new(
        FakeGateway::ready(),
        MemoryRegistry::default(),
        FakeScanner::ready(),
        sessions.clone(),
    );
    facade.scan(request(), session_id()).await.unwrap();

    sessions.clear();

    assert_eq!(
        facade.confirm(&session_id()).await.unwrap_err().code,
        SharedChannelErrorCode::RegistrationSessionNotFound
    );
}
