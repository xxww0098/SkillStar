//! Gemini CLI 切号测试：两个文件的形状、写后回读、导入与回滚材料。
//!
//! 每个写入都落在 temp 目录：路径经 [PathRoots] 注入，测试碰不到真实的
//! `~/.gemini`。

use std::path::{Path, PathBuf};

use crate::oauth::TokenSet;

use super::{accounts_path, credential_path, live_identity, read, snapshot, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::{Sandbox, jwt_with_claims};

/// A temp home plus the roots that resolve inside it.
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

    fn creds_path(&self) -> PathBuf {
        credential_path(&self.roots)
    }
}

fn id_token(email: &str) -> String {
    jwt_with_claims(serde_json::json!({ "email": email }))
}

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: jwt_with_claims(serde_json::json!({
            "sub": format!("sub-{marker}"),
            "exp": 1_800_000_000_i64
        })),
        refresh_token: Some(format!("rt-{marker}")),
        id_token: Some(id_token("dev@example.com")),
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet, email: Option<&'a str>) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "gemini-cli",
        tokens,
        provider_state: None,
        display_name: email.unwrap_or("dev@example.com"),
        account_id: None,
        email,
    }
}

#[test]
fn writes_the_shape_the_cli_reads() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");
    assert_eq!(outcome.agent, "gemini-cli");
    assert_eq!(outcome.live.as_deref(), Some("dev@example.com"));
    assert_eq!(
        outcome.target,
        fixture.home().join("home/.gemini/oauth_creds.json").display().to_string()
    );

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.creds_path()).expect("oauth_creds.json"))
            .expect("json");
    assert_eq!(body["access_token"], tokens.access_token);
    assert_eq!(body["refresh_token"], "rt-one");
    assert_eq!(body["token_type"], "Bearer");
    // Gemini CLI 的过期时间是毫秒。
    assert_eq!(body["expiry_date"], 1_800_000_000_000_i64);

    let accounts: serde_json::Value = serde_json::from_slice(
        &std::fs::read(accounts_path(&fixture.roots)).expect("google_accounts.json"),
    )
    .expect("json");
    assert_eq!(accounts["active"], "dev@example.com");
    assert_eq!(accounts["old"], serde_json::json!([]));
}

#[test]
fn a_second_switch_moves_the_previous_account_into_the_history() {
    let fixture = Fixture::new();
    let first = tokens("one");
    write(&fixture.roots, &request(&first, Some("first@example.com"))).expect("first");

    let second = tokens("two");
    write(&fixture.roots, &request(&second, Some("second@example.com"))).expect("second");

    let accounts: serde_json::Value = serde_json::from_slice(
        &std::fs::read(accounts_path(&fixture.roots)).expect("google_accounts.json"),
    )
    .expect("json");
    assert_eq!(accounts["active"], "second@example.com");
    assert_eq!(accounts["old"], serde_json::json!(["first@example.com"]));
}

#[test]
fn unknown_fields_the_cli_added_survive_a_switch() {
    let fixture = Fixture::new();
    let path = fixture.creds_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(
        &path,
        r#"{"access_token":"old","refresh_token":"old-rt","scope":"openid cloud-platform","universe_domain":"googleapis.com"}"#,
    )
    .expect("seed");

    // 没有 email 的账号（id token 与显示名都没带）：不能凭空发明一个 active。
    let mut tokens = tokens("one");
    tokens.id_token = None;
    let unnamed = SwitchRequest {
        display_name: "Gemini 账号",
        ..request(&tokens, None)
    };
    write(&fixture.roots, &unnamed).expect("write");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(body["scope"], "openid cloud-platform", "the CLI's own scope survives");
    assert_eq!(body["universe_domain"], "googleapis.com");
    assert_eq!(body["refresh_token"], "rt-one");
    assert!(
        !accounts_path(&fixture.roots).exists(),
        "no account file is invented without an email"
    );
}

#[test]
fn a_switch_without_a_refresh_token_is_refused_before_any_write() {
    let fixture = Fixture::new();
    let mut tokens = tokens("one");
    tokens.refresh_token = None;
    let error = write(&fixture.roots, &request(&tokens, None)).expect_err("no refresh token");
    assert!(error.to_string().contains("refresh token"), "{error}");
    assert!(read(&fixture.roots).expect("read").is_none(), "nothing was written");
}

#[test]
fn reads_back_the_credential_and_its_identity() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");

    let local = read(&fixture.roots).expect("read").expect("credential");
    assert_eq!(local.tokens.access_token, tokens.access_token);
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-one"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.source, fixture.creds_path().display().to_string());

    let live = switch::live_with(&fixture.roots, "gemini-cli").expect("live");
    assert_eq!(live.identity.as_deref(), Some("dev@example.com"));
    assert_eq!(
        live.access_fingerprint,
        Some(crate::identity::fingerprint(&tokens.access_token))
    );
    assert_eq!(
        live.refresh_fingerprint,
        Some(crate::identity::fingerprint("rt-one"))
    );
}

#[test]
fn a_credential_file_without_an_access_token_is_not_an_account() {
    let fixture = Fixture::new();
    let path = fixture.creds_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"refresh_token":"rt"}"#).expect("seed");
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn the_snapshot_puts_both_files_back() {
    let fixture = Fixture::new();
    let path = fixture.creds_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"access_token":"previous"}"#).expect("seed previous");
    let accounts = accounts_path(&fixture.roots);
    std::fs::write(&accounts, r#"{"active":"previous@example.com","old":[]}"#).expect("seed accounts");

    let snapshot = snapshot(&fixture.roots).expect("snapshot");
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");
    switch::restore(&snapshot).expect("restore");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(body["access_token"], "previous");
    let restored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&accounts).expect("accounts")).expect("json");
    assert_eq!(restored["active"], "previous@example.com");
}

#[test]
fn the_snapshot_removes_files_that_did_not_exist_before() {
    let fixture = Fixture::new();
    let snapshot = snapshot(&fixture.roots).expect("snapshot");
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");
    assert!(fixture.creds_path().is_file());

    switch::restore(&snapshot).expect("restore");
    assert!(!fixture.creds_path().exists(), "a rollback leaves no file behind");
    assert!(!accounts_path(&fixture.roots).exists());
}

#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    let tokens = tokens("one");
    let outcome = write(&roots, &request(&tokens, Some("dev@example.com"))).expect("write");
    assert!(
        PathBuf::from(&outcome.target).starts_with(sandbox.root()),
        "{} escaped the sandbox",
        outcome.target
    );
}

#[test]
fn live_identity_is_the_email_the_accounts_file_names() {
    let identity = crate::oauth::Identity {
        email: Some("dev@example.com".to_string()),
        ..crate::oauth::Identity::default()
    };
    assert_eq!(live_identity(&identity).as_deref(), Some("dev@example.com"));
    assert_eq!(live_identity(&crate::oauth::Identity::default()), None);
}
