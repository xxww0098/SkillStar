use super::*;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeGateway {
    access: Arc<Mutex<VecDeque<Option<EffectiveRepositoryAccess>>>>,
    effective_access_error: Arc<Mutex<Option<SharedChannelErrorCode>>>,
    create_outcomes: Arc<Mutex<VecDeque<Result<RemoteInvitationOutcome, SharedChannelError>>>>,
    invitations: Arc<Mutex<Vec<RemoteChannelInvitation>>>,
    invite_calls: Arc<Mutex<Vec<(String, ChannelInviteRole)>>>,
    cancel_calls: Arc<Mutex<Vec<u64>>>,
    remove_calls: Arc<Mutex<Vec<String>>>,
    accept_calls: Arc<Mutex<Vec<u64>>>,
    accept_outcomes: Arc<Mutex<VecDeque<Result<(), SharedChannelError>>>>,
    repository: RemoteRepository,
}

impl FakeGateway {
    fn new() -> Self {
        Self {
            access: Arc::new(Mutex::new(VecDeque::new())),
            effective_access_error: Arc::new(Mutex::new(None)),
            create_outcomes: Arc::new(Mutex::new(VecDeque::new())),
            invitations: Arc::new(Mutex::new(Vec::new())),
            invite_calls: Arc::new(Mutex::new(Vec::new())),
            cancel_calls: Arc::new(Mutex::new(Vec::new())),
            remove_calls: Arc::new(Mutex::new(Vec::new())),
            accept_calls: Arc::new(Mutex::new(Vec::new())),
            accept_outcomes: Arc::new(Mutex::new(VecDeque::new())),
            repository: repository(SharedChannelRole::Owner),
        }
    }
}

#[async_trait]
impl SharedChannelGateway for FakeGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        Ok(vec![GitHubOrganization {
            id: 7,
            login: "acme".into(),
            avatar_url: None,
            viewer_is_admin: true,
        }])
    }

    async fn list_selected_repositories(
        &self,
        _organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        unreachable!()
    }

    async fn create_private_repository(
        &self,
        _organization: &str,
        _name: &str,
        _description: &str,
    ) -> Result<RemoteRepository, SharedChannelError> {
        unreachable!()
    }

    async fn validate_selected_installation(
        &self,
        _organization_id: u64,
    ) -> Result<(), SharedChannelError> {
        unreachable!()
    }

    async fn get_selected_repository(
        &self,
        _organization_id: u64,
        _repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        Ok(self.repository.clone())
    }
}

#[async_trait]
impl ChannelMembershipGateway for FakeGateway {
    async fn effective_access(
        &self,
        _repository: &RemoteRepository,
        _username: &str,
    ) -> Result<Option<EffectiveRepositoryAccess>, SharedChannelError> {
        if let Some(code) = *self.effective_access_error.lock().unwrap() {
            return Err(SharedChannelError::new(
                code,
                "effective access unavailable",
            ));
        }
        Ok(self.access.lock().unwrap().pop_front().flatten())
    }

    async fn list_members(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<Vec<ChannelMember>, SharedChannelError> {
        Ok(Vec::new())
    }

    async fn list_repository_invitations(
        &self,
        _repository: &RemoteRepository,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError> {
        Ok(self.invitations.lock().unwrap().clone())
    }

    async fn create_invitation(
        &self,
        _repository: &RemoteRepository,
        username: &str,
        role: ChannelInviteRole,
    ) -> Result<RemoteInvitationOutcome, SharedChannelError> {
        self.invite_calls
            .lock()
            .unwrap()
            .push((username.into(), role));
        self.create_outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Ok(RemoteInvitationOutcome::Pending(Box::new(invitation(
                    10, username, role,
                ))))
            })
    }

    async fn remove_direct_collaborator(
        &self,
        _repository: &RemoteRepository,
        username: &str,
    ) -> Result<(), SharedChannelError> {
        self.remove_calls.lock().unwrap().push(username.to_string());
        Ok(())
    }

    async fn cancel_invitation(
        &self,
        _repository: &RemoteRepository,
        invitation_id: u64,
    ) -> Result<(), SharedChannelError> {
        self.cancel_calls.lock().unwrap().push(invitation_id);
        Ok(())
    }

    async fn list_user_invitations(
        &self,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError> {
        Ok(self.invitations.lock().unwrap().clone())
    }

    async fn accept_invitation(&self, invitation_id: u64) -> Result<(), SharedChannelError> {
        self.accept_calls.lock().unwrap().push(invitation_id);
        self.accept_outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Ok(()))
    }

    async fn decline_invitation(&self, _invitation_id: u64) -> Result<(), SharedChannelError> {
        Ok(())
    }

    async fn get_repository_by_id(
        &self,
        _repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        Ok(self.repository.clone())
    }
}

#[tokio::test]
async fn removing_a_direct_member_reports_fully_revoked_after_effective_access_disappears() {
    let (facade, gateway, _) = fixture();

    let result = facade.revoke_member(42, "bob").await.unwrap();

    assert_eq!(gateway.remove_calls.lock().unwrap().as_slice(), &["bob"]);
    assert_eq!(result.status, ChannelMemberRevocationStatus::Revoked);
    assert_eq!(result.effective_role, None);
}

#[tokio::test]
async fn removing_a_direct_member_reports_access_that_remains_through_github() {
    let (facade, gateway, _) = fixture();
    gateway
        .access
        .lock()
        .unwrap()
        .push_back(Some(EffectiveRepositoryAccess {
            role: SharedChannelRole::Subscriber,
            source: RepositoryAccessSource::Inherited,
        }));

    let result = facade.revoke_member(42, "bob").await.unwrap();

    assert_eq!(result.status, ChannelMemberRevocationStatus::AccessRemains);
    assert_eq!(result.effective_role, Some(SharedChannelRole::Subscriber));
    assert_eq!(
        result.access_source,
        Some(RepositoryAccessSource::Inherited)
    );
}

#[tokio::test]
async fn temporary_effective_access_errors_never_claim_that_revocation_completed() {
    let (facade, gateway, _) = fixture();
    *gateway.effective_access_error.lock().unwrap() = Some(SharedChannelErrorCode::Network);

    let error = facade.revoke_member(42, "bob").await.unwrap_err();

    assert_eq!(error.code, SharedChannelErrorCode::Network);
    assert!(
        error
            .message
            .contains("could not verify effective GitHub access")
    );
    assert_eq!(gateway.remove_calls.lock().unwrap().as_slice(), &["bob"]);
}

#[derive(Clone)]
struct MemoryRegistry(Arc<Mutex<super::super::SharedChannelStore>>);

#[async_trait]
impl SharedChannelRegistry for MemoryRegistry {
    fn load(&self) -> Result<super::super::SharedChannelStore, SharedChannelError> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn save(&self, store: &super::super::SharedChannelStore) -> Result<(), SharedChannelError> {
        *self.0.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[derive(Clone)]
struct FailingRegistry {
    store: Arc<Mutex<super::super::SharedChannelStore>>,
    saves: Arc<Mutex<usize>>,
    fail_on_save: usize,
}

#[async_trait]
impl SharedChannelRegistry for FailingRegistry {
    fn load(&self) -> Result<super::super::SharedChannelStore, SharedChannelError> {
        Ok(self.store.lock().unwrap().clone())
    }

    fn save(&self, store: &super::super::SharedChannelStore) -> Result<(), SharedChannelError> {
        let mut saves = self.saves.lock().unwrap();
        *saves += 1;
        if *saves == self.fail_on_save {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Storage,
                "injected save failure",
            ));
        }
        *self.store.lock().unwrap() = store.clone();
        Ok(())
    }
}

#[tokio::test]
async fn subscriber_invitation_uses_pull_and_returns_pending() {
    let (facade, gateway, _) = fixture();
    let result = facade
        .invite(CreateChannelInvitationRequest {
            repository_id: 42,
            username: "bob".into(),
            role: ChannelInviteRole::Subscriber,
        })
        .await
        .unwrap();
    assert_eq!(ChannelInviteRole::Subscriber.github_permission(), "pull");
    assert_eq!(result.status, ChannelMembershipStatus::Pending);
    assert_eq!(result.invitation_id, Some(10));
    assert_eq!(
        gateway.invite_calls.lock().unwrap().as_slice(),
        &[("bob".into(), ChannelInviteRole::Subscriber)]
    );
}

#[tokio::test]
async fn direct_or_inherited_access_never_creates_a_redundant_invitation() {
    for source in [
        RepositoryAccessSource::Direct,
        RepositoryAccessSource::Inherited,
    ] {
        let (facade, gateway, _) = fixture();
        gateway
            .access
            .lock()
            .unwrap()
            .push_back(Some(EffectiveRepositoryAccess {
                role: SharedChannelRole::Subscriber,
                source,
            }));
        let result = facade
            .invite(CreateChannelInvitationRequest {
                repository_id: 42,
                username: "bob".into(),
                role: ChannelInviteRole::Subscriber,
            })
            .await
            .unwrap();
        assert_eq!(result.status, ChannelMembershipStatus::Accepted);
        assert_eq!(result.role, ChannelInviteRole::Subscriber);
        assert!(gateway.invite_calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn an_existing_pending_invitation_is_returned_without_a_duplicate_put() {
    let (facade, gateway, _) = fixture();
    gateway
        .invitations
        .lock()
        .unwrap()
        .push(invitation(11, "Bob", ChannelInviteRole::Subscriber));
    let result = facade
        .invite(CreateChannelInvitationRequest {
            repository_id: 42,
            username: "bob".into(),
            role: ChannelInviteRole::Publisher,
        })
        .await
        .unwrap();
    assert_eq!(result.status, ChannelMembershipStatus::Pending);
    assert_eq!(result.invitation_id, Some(11));
    assert_eq!(result.role, ChannelInviteRole::Subscriber);
    assert!(gateway.invite_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn put_race_reports_the_effective_github_role_instead_of_the_requested_role() {
    let (facade, gateway, _) = fixture();
    gateway.create_outcomes.lock().unwrap().push_back(Ok(
        RemoteInvitationOutcome::AlreadyAccepted(EffectiveRepositoryAccess {
            role: SharedChannelRole::Subscriber,
            source: RepositoryAccessSource::Inherited,
        }),
    ));
    let result = facade
        .invite(CreateChannelInvitationRequest {
            repository_id: 42,
            username: "bob".into(),
            role: ChannelInviteRole::Publisher,
        })
        .await
        .unwrap();
    assert_eq!(result.status, ChannelMembershipStatus::Accepted);
    assert_eq!(result.role, ChannelInviteRole::Subscriber);
}

#[tokio::test]
async fn resend_cancels_the_exact_pending_invitation_before_recreating_it() {
    let (facade, gateway, _) = fixture();
    gateway
        .invitations
        .lock()
        .unwrap()
        .push(invitation(11, "bob", ChannelInviteRole::Publisher));
    let result = facade.resend(42, 11).await.unwrap();
    assert_eq!(gateway.cancel_calls.lock().unwrap().as_slice(), &[11]);
    assert_eq!(result.status, ChannelMembershipStatus::Pending);
    assert_eq!(
        gateway.invite_calls.lock().unwrap().as_slice(),
        &[("bob".into(), ChannelInviteRole::Publisher)]
    );
}

#[tokio::test]
async fn resend_never_silently_downgrades_an_external_admin_invitation() {
    let (facade, gateway, _) = fixture();
    let mut admin = invitation(11, "bob", ChannelInviteRole::Publisher);
    admin.invitation.effective_role = SharedChannelRole::Owner;
    gateway.invitations.lock().unwrap().push(admin);
    let error = facade.resend(42, 11).await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::InvitationValidation);
    assert!(gateway.cancel_calls.lock().unwrap().is_empty());
    assert!(gateway.invite_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn accepting_an_invitation_registers_the_channel_with_the_effective_role() {
    let (_facade, gateway, registry) = fixture();
    gateway
        .invitations
        .lock()
        .unwrap()
        .push(invitation(12, "alice", ChannelInviteRole::Publisher));
    let facade = ChannelMembershipFacade::new(gateway, registry.clone());
    let descriptor = facade.accept(12).await.unwrap();
    assert_eq!(descriptor.role, SharedChannelRole::Publisher);
    assert_eq!(descriptor.status, SharedChannelStatus::Active);
    assert_eq!(registry.load().unwrap().channels, vec![descriptor]);
}

#[tokio::test]
async fn accepted_remote_invitation_keeps_a_recoverable_marker_when_final_save_fails() {
    let gateway = FakeGateway::new();
    gateway
        .invitations
        .lock()
        .unwrap()
        .push(invitation(12, "alice", ChannelInviteRole::Publisher));
    let store = Arc::new(Mutex::new(super::super::SharedChannelStore::default()));
    let failing = FailingRegistry {
        store: store.clone(),
        saves: Arc::new(Mutex::new(0)),
        fail_on_save: 2,
    };
    let error = ChannelMembershipFacade::new(gateway.clone(), failing)
        .accept(12)
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(error.message.contains("Retry accepted invitation import"));
    assert_eq!(
        store.lock().unwrap().channels[0].status,
        SharedChannelStatus::AwaitingInvitationAcceptance
    );

    let registry = MemoryRegistry(store);
    let mut renamed_repository = gateway.repository.clone();
    renamed_repository.owner_login = "acme-renamed".into();
    renamed_repository.name = "channel-renamed".into();
    renamed_repository.html_url = "https://github.com/acme-renamed/channel-renamed".into();
    renamed_repository.clone_url = "https://github.com/acme-renamed/channel-renamed.git".into();
    let recovery_gateway = FakeGateway {
        repository: renamed_repository,
        ..gateway
    };
    let descriptor = ChannelMembershipFacade::new(recovery_gateway, registry.clone())
        .resume_accepted_channel(42)
        .await
        .unwrap();
    assert_eq!(descriptor.status, SharedChannelStatus::Active);
    assert_eq!(descriptor.owner, "acme-renamed");
    assert_eq!(descriptor.name, "channel-renamed");
    assert_eq!(registry.load().unwrap().channels, vec![descriptor]);
}

#[tokio::test]
async fn invitation_is_not_consumed_when_the_recovery_marker_cannot_be_saved() {
    let gateway = FakeGateway::new();
    gateway.invitations.lock().unwrap().push(invitation(
        12,
        "alice",
        ChannelInviteRole::Subscriber,
    ));
    let failing = FailingRegistry {
        store: Arc::new(Mutex::new(super::super::SharedChannelStore::default())),
        saves: Arc::new(Mutex::new(0)),
        fail_on_save: 1,
    };
    let error = ChannelMembershipFacade::new(gateway.clone(), failing)
        .accept(12)
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Storage);
    assert!(gateway.accept_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn uncertain_github_acceptance_keeps_the_recovery_marker() {
    let gateway = FakeGateway::new();
    gateway.invitations.lock().unwrap().push(invitation(
        12,
        "alice",
        ChannelInviteRole::Subscriber,
    ));
    gateway
        .accept_outcomes
        .lock()
        .unwrap()
        .push_back(Err(SharedChannelError::new(
            SharedChannelErrorCode::Network,
            "response lost",
        )));
    let registry = MemoryRegistry(Arc::new(Mutex::new(
        super::super::SharedChannelStore::default(),
    )));
    let error = ChannelMembershipFacade::new(gateway, registry.clone())
        .accept(12)
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::Network);
    assert!(error.message.contains("uncertain acceptance outcome"));
    assert_eq!(
        registry.load().unwrap().channels[0].status,
        SharedChannelStatus::AwaitingInvitationAcceptance
    );
}

#[tokio::test]
async fn definitive_github_rejection_removes_the_recovery_marker() {
    let gateway = FakeGateway::new();
    gateway.invitations.lock().unwrap().push(invitation(
        12,
        "alice",
        ChannelInviteRole::Subscriber,
    ));
    gateway
        .accept_outcomes
        .lock()
        .unwrap()
        .push_back(Err(SharedChannelError::new(
            SharedChannelErrorCode::InvitationValidation,
            "definitive rejection",
        )));
    let registry = MemoryRegistry(Arc::new(Mutex::new(
        super::super::SharedChannelStore::default(),
    )));
    let error = ChannelMembershipFacade::new(gateway, registry.clone())
        .accept(12)
        .await
        .unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::InvitationValidation);
    assert!(registry.load().unwrap().channels.is_empty());
}

#[tokio::test]
async fn declining_after_an_uncertain_accept_removes_the_local_recovery_marker_first() {
    let gateway = FakeGateway::new();
    gateway.invitations.lock().unwrap().push(invitation(
        12,
        "alice",
        ChannelInviteRole::Subscriber,
    ));
    let mut pending = descriptor();
    pending.status = SharedChannelStatus::AwaitingInvitationAcceptance;
    pending.role = SharedChannelRole::Subscriber;
    let registry = MemoryRegistry(Arc::new(Mutex::new(super::super::SharedChannelStore {
        schema_version: super::super::SHARED_CHANNEL_STORE_VERSION,
        channels: vec![pending],
    })));
    let result = ChannelMembershipFacade::new(gateway, registry.clone())
        .decline(12)
        .await
        .unwrap();
    assert_eq!(result.status, ChannelMembershipStatus::Cancelled);
    assert!(registry.load().unwrap().channels.is_empty());
}

#[tokio::test]
async fn every_classified_invitation_error_is_preserved_by_the_facade() {
    let codes = [
        SharedChannelErrorCode::InvitationOrganizationPolicy,
        SharedChannelErrorCode::InvitationSsoRequired,
        SharedChannelErrorCode::InvitationTwoFactorRequired,
        SharedChannelErrorCode::InvitationSeatUnavailable,
        SharedChannelErrorCode::InvitationValidation,
        SharedChannelErrorCode::InvitationRateLimited,
        SharedChannelErrorCode::InvitationLimit,
    ];
    for code in codes {
        let (facade, gateway, _) = fixture();
        gateway
            .create_outcomes
            .lock()
            .unwrap()
            .push_back(Err(SharedChannelError::new(
                code,
                "classified by GitHub gateway",
            )));
        let error = facade
            .invite(CreateChannelInvitationRequest {
                repository_id: 42,
                username: "bob".into(),
                role: ChannelInviteRole::Subscriber,
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
    }
}

#[tokio::test]
async fn current_admin_permission_is_required_even_for_a_stale_owner_descriptor() {
    let (_facade, gateway, registry) = fixture();
    let mut downgraded = repository(SharedChannelRole::Subscriber);
    downgraded.permissions = permissions_for_role(SharedChannelRole::Subscriber);
    let facade = ChannelMembershipFacade::new(
        FakeGateway {
            repository: downgraded,
            ..gateway
        },
        registry,
    );
    let error = facade.list_membership(42).await.unwrap_err();
    assert_eq!(error.code, SharedChannelErrorCode::PermissionDenied);
}

fn fixture() -> (
    ChannelMembershipFacade<FakeGateway, MemoryRegistry>,
    FakeGateway,
    MemoryRegistry,
) {
    let gateway = FakeGateway::new();
    let registry = MemoryRegistry(Arc::new(Mutex::new(super::super::SharedChannelStore {
        schema_version: super::super::SHARED_CHANNEL_STORE_VERSION,
        channels: vec![descriptor()],
    })));
    (
        ChannelMembershipFacade::new(gateway.clone(), registry.clone()),
        gateway,
        registry,
    )
}

fn descriptor() -> SharedChannelDescriptor {
    SharedChannelDescriptor {
        descriptor_version: CHANNEL_DESCRIPTOR_VERSION,
        repository_id: 42,
        organization_id: 7,
        owner: "acme".into(),
        name: "channel".into(),
        html_url: "https://github.com/acme/channel".into(),
        clone_url: "https://github.com/acme/channel.git".into(),
        role: SharedChannelRole::Owner,
        status: SharedChannelStatus::Active,
        authorization: SharedChannelAuthorization::default(),
        created_at: "2026-08-05T00:00:00Z".into(),
        updated_at: "2026-08-05T00:00:00Z".into(),
    }
}

fn repository(role: SharedChannelRole) -> RemoteRepository {
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
        permissions: permissions_for_role(role),
    }
}

fn invitation(id: u64, username: &str, role: ChannelInviteRole) -> RemoteChannelInvitation {
    RemoteChannelInvitation {
        invitation: ChannelInvitation {
            id,
            repository_id: 42,
            organization_id: 7,
            owner: "acme".into(),
            repository_name: "channel".into(),
            html_url: "https://github.com/acme/channel".into(),
            invitee: Some(ChannelMemberIdentity {
                id: 8,
                login: username.into(),
            }),
            inviter: Some(ChannelMemberIdentity {
                id: 9,
                login: "alice".into(),
            }),
            role,
            effective_role: match role {
                ChannelInviteRole::Subscriber => SharedChannelRole::Subscriber,
                ChannelInviteRole::Publisher => SharedChannelRole::Publisher,
            },
            status: ChannelMembershipStatus::Pending,
            created_at: "2026-08-05T00:00:00Z".into(),
        },
        repository: repository(match role {
            ChannelInviteRole::Subscriber => SharedChannelRole::Subscriber,
            ChannelInviteRole::Publisher => SharedChannelRole::Publisher,
        }),
    }
}
