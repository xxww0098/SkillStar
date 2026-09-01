use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use skillstar_git::transport::{GitOperationSession, NoopGitProgressSink, classify_git_failure};
use tokio::sync::Notify;

use super::{
    Clock, CredentialStore, DeviceAuthorizationGrant, GatewayDevicePoll, GitHubAuthError,
    GitHubAuthFacade, GitHubGateway, GitHubHttpGateway, GitHubHttpResponse, GitHubHttpTransport,
    GitHubIdentity, StoredCredential, TokenGrant,
};

#[derive(Clone)]
struct TestClock(Arc<Mutex<DateTime<Utc>>>);

impl TestClock {
    fn at(value: &str) -> Self {
        Self(Arc::new(Mutex::new(parse_time(value))))
    }

    fn set(&self, value: &str) {
        *self.0.lock().expect("clock lock") = parse_time(value);
    }
}

fn parse_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("valid fixture time")
        .with_timezone(&Utc)
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

#[derive(Clone, Default)]
struct MemoryCredentials(Arc<Mutex<Option<StoredCredential>>>);

impl CredentialStore for MemoryCredentials {
    fn load(&self) -> Result<Option<StoredCredential>, GitHubAuthError> {
        Ok(self.0.lock().expect("credential lock").clone())
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), GitHubAuthError> {
        *self.0.lock().expect("credential lock") = Some(credential.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), GitHubAuthError> {
        *self.0.lock().expect("credential lock") = None;
        Ok(())
    }
}

#[derive(Clone)]
struct FakeGateway {
    poll_result: Arc<Mutex<GatewayDevicePoll>>,
    refresh_result: Arc<Mutex<Option<TokenGrant>>>,
    refreshed_with: Arc<Mutex<Vec<String>>>,
}

impl FakeGateway {
    fn authorized() -> Self {
        Self {
            poll_result: Arc::new(Mutex::new(GatewayDevicePoll::Authorized(TokenGrant {
                access_token: "access-secret".into(),
                refresh_token: Some("refresh-secret".into()),
                access_expires_in_seconds: Some(3_600),
                refresh_expires_in_seconds: Some(86_400),
            }))),
            refresh_result: Arc::new(Mutex::new(Some(TokenGrant {
                access_token: "refreshed-access".into(),
                refresh_token: None,
                access_expires_in_seconds: Some(120),
                refresh_expires_in_seconds: None,
            }))),
            refreshed_with: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn set_poll_result(&self, result: GatewayDevicePoll) {
        *self.poll_result.lock().expect("poll lock") = result;
    }
}

#[async_trait]
impl GitHubGateway for FakeGateway {
    async fn start_device_authorization(
        &self,
    ) -> Result<DeviceAuthorizationGrant, GitHubAuthError> {
        Ok(DeviceAuthorizationGrant {
            device_code: "device-secret".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in_seconds: 900,
            interval_seconds: 5,
        })
    }

    async fn poll_device_authorization(
        &self,
        _device_code: &str,
    ) -> Result<GatewayDevicePoll, GitHubAuthError> {
        Ok(self.poll_result.lock().expect("poll lock").clone())
    }

    async fn refresh_access_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenGrant, GitHubAuthError> {
        self.refreshed_with
            .lock()
            .expect("refresh log lock")
            .push(refresh_token.to_string());
        self.refresh_result
            .lock()
            .expect("refresh result lock")
            .clone()
            .ok_or_else(|| {
                GitHubAuthError::new(
                    super::GitHubAuthErrorCode::Network,
                    "Unable to reach GitHub",
                )
            })
    }

    async fn current_user(&self, access_token: &str) -> Result<GitHubIdentity, GitHubAuthError> {
        assert!(matches!(access_token, "access-secret" | "refreshed-access"));
        Ok(GitHubIdentity {
            id: 42,
            login: "octocat".into(),
            avatar_url: Some("https://avatars.example/octocat".into()),
        })
    }
}

#[derive(Clone)]
struct BlockingGateway {
    poll_entered: Arc<Notify>,
    poll_release: Arc<Notify>,
    refresh_entered: Arc<Notify>,
    refresh_release: Arc<Notify>,
}

impl BlockingGateway {
    fn new() -> Self {
        Self {
            poll_entered: Arc::new(Notify::new()),
            poll_release: Arc::new(Notify::new()),
            refresh_entered: Arc::new(Notify::new()),
            refresh_release: Arc::new(Notify::new()),
        }
    }

    fn token(access_token: &str) -> TokenGrant {
        TokenGrant {
            access_token: access_token.into(),
            refresh_token: Some("refresh-secret".into()),
            access_expires_in_seconds: Some(3_600),
            refresh_expires_in_seconds: Some(86_400),
        }
    }
}

#[async_trait]
impl GitHubGateway for BlockingGateway {
    async fn start_device_authorization(
        &self,
    ) -> Result<DeviceAuthorizationGrant, GitHubAuthError> {
        Ok(DeviceAuthorizationGrant {
            device_code: "device-secret".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in_seconds: 900,
            interval_seconds: 5,
        })
    }

    async fn poll_device_authorization(
        &self,
        _device_code: &str,
    ) -> Result<GatewayDevicePoll, GitHubAuthError> {
        self.poll_entered.notify_one();
        self.poll_release.notified().await;
        Ok(GatewayDevicePoll::Authorized(Self::token("access-secret")))
    }

    async fn refresh_access_token(
        &self,
        _refresh_token: &str,
    ) -> Result<TokenGrant, GitHubAuthError> {
        self.refresh_entered.notify_one();
        self.refresh_release.notified().await;
        Ok(Self::token("refreshed-access"))
    }

    async fn current_user(&self, _access_token: &str) -> Result<GitHubIdentity, GitHubAuthError> {
        Ok(GitHubIdentity {
            id: 42,
            login: "octocat".into(),
            avatar_url: None,
        })
    }
}

#[tokio::test]
async fn provider_expiry_drives_status_refresh_and_logout() {
    let gateway = FakeGateway::authorized();
    let credentials = MemoryCredentials::default();
    let clock = TestClock::at("2026-08-05T10:00:00Z");
    let facade = GitHubAuthFacade::new(gateway.clone(), credentials.clone(), clock.clone());
    facade.start_device_flow().await.expect("start");
    facade.poll_device_flow().await.expect("authorize");

    clock.set("2026-08-05T11:00:00Z");
    assert!(matches!(
        facade.status().await.expect("status"),
        super::GitHubConnectionStatus::Expired { .. }
    ));

    let refreshed = facade.refresh().await.expect("refresh");
    match refreshed {
        super::GitHubConnectionStatus::Connected {
            identity,
            access_expires_at,
        } => {
            assert_eq!(identity.login, "octocat");
            assert_eq!(
                access_expires_at.map(|value| value.to_rfc3339()),
                Some("2026-08-05T11:02:00+00:00".into())
            );
        }
        other => panic!("expected connected, got {other:?}"),
    }
    assert_eq!(
        gateway
            .refreshed_with
            .lock()
            .expect("refresh log")
            .as_slice(),
        ["refresh-secret"]
    );
    let stored = credentials.load().expect("load").expect("saved");
    assert_eq!(stored.access_token(), "refreshed-access");
    assert_eq!(stored.refresh_token(), Some("refresh-secret"));
    assert_eq!(
        stored.refresh_expires_at().map(|value| value.to_rfc3339()),
        Some("2026-08-06T10:00:00+00:00".into())
    );

    facade.logout().expect("logout");
    assert!(matches!(
        facade.status().await.expect("signed out status"),
        super::GitHubConnectionStatus::SignedOut
    ));
    assert!(credentials.load().expect("load after logout").is_none());
}

#[tokio::test]
async fn git_auth_material_tracks_file_store_presence_and_provider_expiry_without_a_dto() {
    let gateway = FakeGateway::authorized();
    let credentials = MemoryCredentials::default();
    let clock = TestClock::at("2026-08-05T10:00:00Z");
    let facade = GitHubAuthFacade::new(gateway, credentials, clock.clone());

    let signed_out = GitOperationSession::new(
        "signed-out",
        facade.git_auth_material().expect("signed out material"),
        Arc::new(NoopGitProgressSink),
    );
    assert!(!signed_out.has_credential());
    assert_eq!(
        classify_git_failure("Authentication failed", &signed_out)
            .code
            .as_str(),
        "not_authenticated"
    );

    facade.start_device_flow().await.expect("start");
    facade.poll_device_flow().await.expect("authorize");
    let connected = GitOperationSession::new(
        "connected",
        facade.git_auth_material().expect("connected material"),
        Arc::new(NoopGitProgressSink),
    );
    assert!(connected.has_credential());
    let api_credential = facade.api_credential().expect("opaque API credential");
    assert!(!format!("{api_credential:?}").contains("access-secret"));

    clock.set("2026-08-05T11:00:00Z");
    let expired = GitOperationSession::new(
        "expired",
        facade.git_auth_material().expect("expired material"),
        Arc::new(NoopGitProgressSink),
    );
    assert!(!expired.has_credential());
    assert_eq!(
        classify_git_failure("Authentication failed", &expired)
            .code
            .as_str(),
        "token_expired"
    );
}

#[tokio::test]
async fn status_restores_identity_through_the_gateway_without_persisting_it_as_public_state() {
    let credentials = MemoryCredentials::default();
    let clock = TestClock::at("2026-08-05T10:00:00Z");
    let first = GitHubAuthFacade::new(
        FakeGateway::authorized(),
        credentials.clone(),
        clock.clone(),
    );
    first.start_device_flow().await.expect("start");
    first.poll_device_flow().await.expect("authorize");

    let restarted = GitHubAuthFacade::new(FakeGateway::authorized(), credentials, clock);
    assert!(matches!(
        restarted.status().await.expect("restored status"),
        super::GitHubConnectionStatus::Connected { identity, .. } if identity.login == "octocat"
    ));
}

#[tokio::test]
async fn device_flow_pending_slow_down_cancel_and_terminal_states_are_structured() {
    let gateway = FakeGateway::authorized();
    let facade = GitHubAuthFacade::new(
        gateway.clone(),
        MemoryCredentials::default(),
        TestClock::at("2026-08-05T10:00:00Z"),
    );
    gateway.set_poll_result(GatewayDevicePoll::Pending);
    facade.start_device_flow().await.expect("start");
    assert_eq!(
        facade.poll_device_flow().await.expect("pending"),
        super::DeviceFlowPoll::Pending {
            retry_after_seconds: 5
        }
    );

    gateway.set_poll_result(GatewayDevicePoll::SlowDown);
    assert_eq!(
        facade.poll_device_flow().await.expect("slow down"),
        super::DeviceFlowPoll::SlowDown {
            retry_after_seconds: 10
        }
    );
    assert!(facade.cancel_device_flow());
    assert!(!facade.cancel_device_flow());

    facade.start_device_flow().await.expect("retry start");
    gateway.set_poll_result(GatewayDevicePoll::Denied);
    assert_eq!(
        facade.poll_device_flow().await.expect("denied"),
        super::DeviceFlowPoll::Denied
    );
    assert_eq!(
        facade
            .poll_device_flow()
            .await
            .expect_err("terminal clears pending")
            .code,
        super::GitHubAuthErrorCode::NoPendingAuthorization
    );
}

#[tokio::test]
async fn device_flow_expires_from_provider_metadata_before_another_poll() {
    let gateway = FakeGateway::authorized();
    let clock = TestClock::at("2026-08-05T10:00:00Z");
    let facade = GitHubAuthFacade::new(gateway, MemoryCredentials::default(), clock.clone());
    facade.start_device_flow().await.expect("start");
    clock.set("2026-08-05T10:15:00Z");

    assert_eq!(
        facade.poll_device_flow().await.expect("locally expired"),
        super::DeviceFlowPoll::Expired
    );
    assert_eq!(
        facade
            .poll_device_flow()
            .await
            .expect_err("pending flow cleared")
            .code,
        super::GitHubAuthErrorCode::NoPendingAuthorization
    );
}

#[tokio::test]
async fn cancelling_an_in_flight_poll_prevents_credentials_from_being_saved() {
    let gateway = BlockingGateway::new();
    let credentials = MemoryCredentials::default();
    let facade = Arc::new(GitHubAuthFacade::new(
        gateway.clone(),
        credentials.clone(),
        TestClock::at("2026-08-05T10:00:00Z"),
    ));
    facade.start_device_flow().await.expect("start");

    let polling_facade = Arc::clone(&facade);
    let poll = tokio::spawn(async move { polling_facade.poll_device_flow().await });
    gateway.poll_entered.notified().await;
    assert!(facade.cancel_device_flow());
    gateway.poll_release.notify_one();

    let error = poll.await.expect("poll task").expect_err("cancelled poll");
    assert_eq!(
        error.code,
        super::GitHubAuthErrorCode::NoPendingAuthorization
    );
    assert!(
        credentials
            .load()
            .expect("credentials remain readable")
            .is_none()
    );
}

#[tokio::test]
async fn logout_during_refresh_prevents_credentials_from_being_restored() {
    let credentials = MemoryCredentials::default();
    let clock = TestClock::at("2026-08-05T10:00:00Z");
    let login = GitHubAuthFacade::new(
        FakeGateway::authorized(),
        credentials.clone(),
        clock.clone(),
    );
    login.start_device_flow().await.expect("start");
    login.poll_device_flow().await.expect("authorize");

    let gateway = BlockingGateway::new();
    let facade = Arc::new(GitHubAuthFacade::new(
        gateway.clone(),
        credentials.clone(),
        clock,
    ));
    let refreshing_facade = Arc::clone(&facade);
    let refresh = tokio::spawn(async move { refreshing_facade.refresh().await });
    gateway.refresh_entered.notified().await;
    facade.logout().expect("logout");
    gateway.refresh_release.notify_one();

    let error = refresh
        .await
        .expect("refresh task")
        .expect_err("logged out refresh");
    assert_eq!(error.code, super::GitHubAuthErrorCode::NotAuthenticated);
    assert!(
        credentials
            .load()
            .expect("credentials remain readable")
            .is_none()
    );
}

#[tokio::test]
async fn device_authorization_exposes_only_user_guidance_then_persists_authorized_tokens() {
    let credentials = MemoryCredentials::default();
    let facade = GitHubAuthFacade::new(
        FakeGateway::authorized(),
        credentials.clone(),
        TestClock::at("2026-08-05T10:00:00Z"),
    );

    let guidance = facade.start_device_flow().await.expect("start device flow");
    assert_eq!(guidance.user_code, "ABCD-EFGH");
    assert_eq!(
        guidance.expires_at.to_rfc3339(),
        "2026-08-05T10:15:00+00:00"
    );
    let serialized = serde_json::to_string(&guidance).expect("serialize public guidance");
    assert!(!serialized.contains("device-secret"));
    assert!(!serialized.contains("access-secret"));

    let outcome = facade.poll_device_flow().await.expect("poll device flow");
    let serialized = serde_json::to_string(&outcome).expect("serialize public outcome");
    assert!(serialized.contains("octocat"));
    assert!(!serialized.contains("access-secret"));
    assert!(!serialized.contains("refresh-secret"));

    let stored = credentials
        .load()
        .expect("read credential")
        .expect("credential saved");
    assert_eq!(stored.access_token(), "access-secret");
    assert_eq!(stored.refresh_token(), Some("refresh-secret"));
    assert_eq!(
        stored.access_expires_at().map(|value| value.to_rfc3339()),
        Some("2026-08-05T11:00:00+00:00".into())
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CapturedHttpCall {
    Form {
        url: String,
        fields: Vec<(String, String)>,
    },
    BearerGet {
        url: String,
        bearer: String,
    },
}

#[derive(Clone)]
struct FixtureTransport {
    responses: Arc<Mutex<VecDeque<GitHubHttpResponse>>>,
    calls: Arc<Mutex<Vec<CapturedHttpCall>>>,
}

impl FixtureTransport {
    fn new(responses: impl IntoIterator<Item = GitHubHttpResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses.into_iter().collect())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl GitHubHttpTransport for FixtureTransport {
    async fn post_form(
        &self,
        url: &str,
        fields: &[(String, String)],
    ) -> Result<GitHubHttpResponse, GitHubAuthError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(CapturedHttpCall::Form {
                url: url.into(),
                fields: fields.to_vec(),
            });
        Ok(self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("fixture response"))
    }

    async fn get_bearer(
        &self,
        url: &str,
        bearer: &str,
    ) -> Result<GitHubHttpResponse, GitHubAuthError> {
        self.calls
            .lock()
            .expect("calls lock")
            .push(CapturedHttpCall::BearerGet {
                url: url.into(),
                bearer: bearer.into(),
            });
        Ok(self
            .responses
            .lock()
            .expect("responses lock")
            .pop_front()
            .expect("fixture response"))
    }
}

fn fixture(status: u16, body: &str) -> GitHubHttpResponse {
    GitHubHttpResponse {
        status,
        body: body.into(),
    }
}

#[tokio::test]
async fn github_gateway_matches_device_flow_contract_and_maps_provider_states() {
    let transport = FixtureTransport::new([
        fixture(
            200,
            r#"{"device_code":"device-secret","user_code":"ABCD-EFGH","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#,
        ),
        fixture(200, r#"{"error":"authorization_pending"}"#),
        fixture(200, r#"{"error":"slow_down","interval":10}"#),
        fixture(
            200,
            r#"{"access_token":"access-secret","token_type":"bearer","expires_in":28800,"refresh_token":"refresh-secret","refresh_token_expires_in":15897600}"#,
        ),
        fixture(
            200,
            r#"{"id":42,"login":"octocat","avatar_url":"https://avatars.example/octocat"}"#,
        ),
    ]);
    let gateway = GitHubHttpGateway::with_transport("client-id", transport.clone());

    let started = gateway
        .start_device_authorization()
        .await
        .expect("device fixture");
    assert_eq!(started.user_code, "ABCD-EFGH");
    assert_eq!(started.expires_in_seconds, 900);
    assert!(matches!(
        gateway
            .poll_device_authorization("device-secret")
            .await
            .expect("pending fixture"),
        GatewayDevicePoll::Pending
    ));
    assert!(matches!(
        gateway
            .poll_device_authorization("device-secret")
            .await
            .expect("slow fixture"),
        GatewayDevicePoll::SlowDown
    ));
    let token = match gateway
        .poll_device_authorization("device-secret")
        .await
        .expect("authorized fixture")
    {
        GatewayDevicePoll::Authorized(token) => token,
        _ => panic!("expected authorized token grant"),
    };
    assert_eq!(token.access_expires_in_seconds, Some(28_800));
    assert_eq!(token.refresh_expires_in_seconds, Some(15_897_600));
    let identity = gateway
        .current_user(&token.access_token)
        .await
        .expect("identity fixture");
    assert_eq!(identity.login, "octocat");

    let calls = transport.calls.lock().expect("calls lock");
    assert_eq!(
        calls[0],
        CapturedHttpCall::Form {
            url: "https://github.com/login/device/code".into(),
            fields: vec![("client_id".into(), "client-id".into())],
        }
    );
    assert_eq!(
        calls[1],
        CapturedHttpCall::Form {
            url: "https://github.com/login/oauth/access_token".into(),
            fields: vec![
                ("client_id".into(), "client-id".into()),
                ("device_code".into(), "device-secret".into()),
                (
                    "grant_type".into(),
                    "urn:ietf:params:oauth:grant-type:device_code".into(),
                ),
            ],
        }
    );
    assert_eq!(
        calls[4],
        CapturedHttpCall::BearerGet {
            url: "https://api.github.com/user".into(),
            bearer: "access-secret".into(),
        }
    );
}

#[tokio::test]
async fn github_gateway_maps_terminal_device_and_protocol_errors_without_echoing_secrets() {
    let transport = FixtureTransport::new([
        fixture(200, r#"{"error":"expired_token"}"#),
        fixture(200, r#"{"error":"access_denied"}"#),
        fixture(
            200,
            r#"{"error":"incorrect_client_credentials","error_description":"bad device-secret"}"#,
        ),
    ]);
    let gateway = GitHubHttpGateway::with_transport("client-id", transport);
    assert!(matches!(
        gateway
            .poll_device_authorization("device-secret")
            .await
            .expect("expired"),
        GatewayDevicePoll::Expired
    ));
    assert!(matches!(
        gateway
            .poll_device_authorization("device-secret")
            .await
            .expect("denied"),
        GatewayDevicePoll::Denied
    ));
    let error = match gateway.poll_device_authorization("device-secret").await {
        Ok(_) => panic!("invalid client must be rejected"),
        Err(error) => error,
    };
    assert_eq!(error.code, super::GitHubAuthErrorCode::Unavailable);
    assert!(!error.to_string().contains("device-secret"));
}

#[tokio::test]
async fn github_gateway_refresh_contract_uses_only_the_refresh_grant_fields() {
    let transport = FixtureTransport::new([fixture(
        200,
        r#"{"access_token":"next-access","token_type":"bearer","expires_in":3600,"refresh_token":"next-refresh","refresh_token_expires_in":7200}"#,
    )]);
    let gateway = GitHubHttpGateway::with_transport("client-id", transport.clone());
    let grant = gateway
        .refresh_access_token("refresh-secret")
        .await
        .expect("refresh fixture");
    assert_eq!(grant.access_token, "next-access");
    assert_eq!(grant.refresh_token.as_deref(), Some("next-refresh"));

    assert_eq!(
        transport.calls.lock().expect("calls lock").as_slice(),
        [CapturedHttpCall::Form {
            url: "https://github.com/login/oauth/access_token".into(),
            fields: vec![
                ("client_id".into(), "client-id".into()),
                ("refresh_token".into(), "refresh-secret".into()),
                ("grant_type".into(), "refresh_token".into()),
            ],
        }]
    );
}
