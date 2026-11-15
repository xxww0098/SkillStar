//! Kiro 切号测试：三处文件的形状、写后回读、IdC 注册与回滚。
//!
//! 路径全部经 PathRoots 注入 temp 目录，碰不到真实的 ~/.aws。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::oauth::agents::kiro::{BUILDER_ID_START_URL, KiroState};
use crate::oauth::TokenSet;

use super::{credential_path, live_identity, read, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::Sandbox;

struct Fixture {
    dir: tempfile::TempDir,
    roots: PathRoots,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        Self { dir, roots }
    }

    fn home(&self) -> &Path {
        self.dir.path()
    }

    fn token_path(&self) -> PathBuf {
        credential_path(&self.roots)
    }
}

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: format!("at-{marker}"),
        refresh_token: Some(format!("rt-{marker}")),
        id_token: None,
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn idc_state() -> Value {
    KiroState {
        client_id: Some("cid".to_string()),
        client_secret: Some("csec".to_string()),
        region: Some("us-east-1".to_string()),
        start_url: Some(BUILDER_ID_START_URL.to_string()),
        profile_arn: Some("arn:aws:codewhisperer:us-east-1:1:profile/ABC".to_string()),
    }
    .to_value()
    .expect("state")
}

fn request<'a>(tokens: &'a TokenSet, state: Option<&'a Value>) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "kiro",
        tokens,
        provider_state: state,
        display_name: "dev@example.com",
        account_id: Some("user-1"),
        email: Some("dev@example.com"),
    }
}

#[test]
fn an_idc_switch_writes_the_login_the_registration_and_the_profile() {
    let fixture = Fixture::new();
    let state = idc_state();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens, Some(&state))).expect("write");
    assert_eq!(outcome.agent, "kiro");
    assert_eq!(outcome.live.as_deref(), Some("user-1"));
    assert_eq!(
        outcome.target,
        fixture
            .home()
            .join("home/.aws/sso/cache/kiro-auth-token.json")
            .display()
            .to_string()
    );

    let token: Value =
        serde_json::from_slice(&std::fs::read(fixture.token_path()).expect("read")).expect("json");
    assert_eq!(token["accessToken"], "at-one");
    assert_eq!(token["refreshToken"], "rt-one");
    assert_eq!(token["clientId"], "cid");
    assert_eq!(token["authMethod"], "IdC");
    assert_eq!(token["email"], "dev@example.com");
    assert!(token.get("clientSecret").is_none(), "the secret stays in the registration");

    let hash = crate::switch::kiro_store::client_id_hash(BUILDER_ID_START_URL);
    let registration = fixture
        .home()
        .join("home/.aws/sso/cache")
        .join(format!("{hash}.json"));
    let document: Value =
        serde_json::from_slice(&std::fs::read(&registration).expect("registration")).expect("json");
    assert_eq!(document["clientId"], "cid");
    assert_eq!(document["clientSecret"], "csec");

    let profile: Value = serde_json::from_slice(
        &std::fs::read(
            fixture
                .home()
                .join("home/Library/Application Support/Kiro/profile.json"),
        )
        .expect("profile"),
    )
    .expect("json");
    assert_eq!(profile["email"], "dev@example.com");
    assert_eq!(profile["loginProvider"], "Enterprise");
}

#[test]
fn a_portal_switch_needs_no_registration_file() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens, None)).expect("write");
    assert_eq!(outcome.live.as_deref(), Some("user-1"));

    let token: Value =
        serde_json::from_slice(&std::fs::read(fixture.token_path()).expect("read")).expect("json");
    assert_eq!(token["authMethod"], "social");
    let cache = fixture.home().join("home/.aws/sso/cache");
    let stray: Vec<_> = std::fs::read_dir(&cache)
        .expect("cache dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name != "kiro-auth-token.json")
        .collect();
    assert!(stray.is_empty(), "unexpected registration files: {stray:?}");
}

#[test]
fn unknown_fields_the_client_added_survive_a_switch() {
    let fixture = Fixture::new();
    let path = fixture.token_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "accessToken": "old",
            "refreshToken": "old-rt",
            "authMethod": "social",
            "machineId": "keep-me"
        }))
        .expect("json"),
    )
    .expect("seed");

    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, None)).expect("write");
    let token: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(token["machineId"], "keep-me");
    assert_eq!(token["accessToken"], "at-one");
}

#[test]
fn a_switch_without_a_refresh_token_is_refused_before_any_write() {
    let fixture = Fixture::new();
    let mut tokens = tokens("one");
    tokens.refresh_token = None;
    let error = write(&fixture.roots, &request(&tokens, None)).expect_err("no refresh token");
    assert!(error.to_string().contains("refresh token"), "{error}");
    assert!(!fixture.token_path().exists());
}

#[test]
fn reads_back_the_login_and_its_identity() {
    let fixture = Fixture::new();
    let state = idc_state();
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, Some(&state))).expect("write");

    let local = read(&fixture.roots).expect("read").expect("login");
    assert_eq!(local.tokens.access_token, "at-one");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-one"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.identity.account_id.as_deref(), Some("user-1"));

    let live = switch::live_with(&fixture.roots, "kiro").expect("live");
    assert_eq!(live.identity.as_deref(), Some("user-1"));
    assert_eq!(
        live.access_fingerprint,
        Some(crate::identity::fingerprint("at-one"))
    );
}

#[test]
fn a_token_file_without_an_access_token_is_not_a_login() {
    let fixture = Fixture::new();
    let path = fixture.token_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"refreshToken":"rt"}"#).expect("seed");
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn the_snapshot_puts_the_three_files_back() {
    let fixture = Fixture::new();
    let path = fixture.token_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"accessToken":"previous"}"#).expect("seed");

    let snapshot = switch::snapshot_with(&fixture.roots, "kiro").expect("snapshot");
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, None)).expect("write");
    // 写入之后现场变了。
    assert_ne!(
        std::fs::read_to_string(&path).expect("read"),
        r#"{"accessToken":"previous"}"#
    );
    switch::restore(&snapshot).expect("restore");
    let restored: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(restored["accessToken"], "previous");
}

#[test]
fn live_identity_prefers_the_provider_account_id() {
    let both = crate::oauth::Identity {
        email: Some("dev@example.com".to_string()),
        account_id: Some("user-1".to_string()),
        ..crate::oauth::Identity::default()
    };
    assert_eq!(live_identity(&both), Some("user-1".to_string()));
    assert_eq!(live_identity(&crate::oauth::Identity::default()), None);
}

#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    let tokens = tokens("one");
    let outcome = write(&roots, &request(&tokens, None)).expect("write");
    assert!(
        PathBuf::from(&outcome.target).starts_with(sandbox.root()),
        "{} escaped the sandbox",
        outcome.target
    );
}
