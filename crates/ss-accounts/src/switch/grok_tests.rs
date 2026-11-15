//! Grok 切号测试：作用域检查、条目形状、换账号清镜像字段、回读与回滚。
//!
//! 一律落在 temp 目录（PathRoots 注入），碰不到真实的 ~/.grok。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::oauth::TokenSet;

use super::{credential_path, live_identity, read, snapshot, write};
use crate::oauth::agents::grok;
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

    fn auth_path(&self) -> PathBuf {
        credential_path(&self.roots)
    }
}

/// 一条带完整 CLI scope 的访问令牌。
fn access_token(subject: &str, email: &str) -> String {
    jwt_with_claims(json!({
        "sub": subject,
        "email": email,
        "scope": format!("openid {GROK_SCOPES}"),
        "exp": 1_800_000_000_i64
    }))
}

const GROK_SCOPES: &str = "grok-cli:access conversations:read conversations:write api:access";

fn tokens(subject: &str, email: &str) -> TokenSet {
    TokenSet {
        access_token: access_token(subject, email),
        refresh_token: Some("rt-1".to_string()),
        id_token: None,
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet, email: &'a str) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "grok",
        tokens,
        provider_state: None,
        display_name: email,
        account_id: None,
        email: Some(email),
    }
}

#[test]
fn writes_the_entry_under_the_scope_key_the_cli_reads() {
    let fixture = Fixture::new();
    let tokens = tokens("user-1", "dev@example.com");
    let outcome = write(&fixture.roots, &request(&tokens, "dev@example.com")).expect("write");
    assert_eq!(outcome.agent, "grok");
    assert_eq!(outcome.live.as_deref(), Some("user-1"));
    assert_eq!(
        outcome.target,
        fixture.home().join("home/.grok/auth.json").display().to_string()
    );

    let body: Value = serde_json::from_slice(&std::fs::read(fixture.auth_path()).expect("read"))
        .expect("json");
    let entry = &body[grok::scope_key()];
    assert_eq!(entry["key"], tokens.access_token);
    assert_eq!(entry["refresh_token"], "rt-1");
    assert_eq!(entry["auth_mode"], "oidc");
    assert_eq!(entry["oidc_issuer"], "https://auth.x.ai");
    assert_eq!(entry["oidc_client_id"], grok::client_id());
    assert_eq!(entry["email"], "dev@example.com");
    assert_eq!(entry["user_id"], "user-1");
    assert_eq!(entry["principal_id"], "user-1");
    assert_eq!(entry["first_name"], "dev");
    // 恢复会话需要的字段一个不能少。
    for field in super::RESTORABLE_FIELDS {
        assert!(
            entry.get(field).and_then(Value::as_str).is_some(),
            "{field} missing from the synthesised entry"
        );
    }
}

#[test]
fn a_token_without_the_cli_scopes_is_refused() {
    let fixture = Fixture::new();
    let mut tokens = tokens("user-1", "dev@example.com");
    tokens.access_token = jwt_with_claims(json!({
        "sub": "user-1",
        "email": "dev@example.com",
        "scope": "openid api:access"
    }));
    let error = write(&fixture.roots, &request(&tokens, "dev@example.com"))
        .expect_err("billing-only scopes cannot drive the CLI");
    let message = error.to_string();
    assert!(message.contains("grok-cli:access"), "{message}");
    assert!(!fixture.auth_path().exists(), "nothing was written");
}

#[test]
fn an_opaque_token_is_not_accused_of_missing_scopes() {
    let fixture = Fixture::new();
    let tokens = TokenSet {
        access_token: "opaque-token".to_string(),
        refresh_token: Some("rt-1".to_string()),
        expires_at: Some(1_800_000_000),
        ..TokenSet::default()
    };
    // 不透明令牌读不到 claim：scope 检查放行，身份字段靠记录（email + 账号 id）。
    let request = SwitchRequest {
        account_id: Some("user-1"),
        ..request(&tokens, "dev@example.com")
    };
    let outcome = write(&fixture.roots, &request).expect("an opaque token is writable");
    assert_eq!(outcome.live.as_deref(), Some("user-1"));
}

#[test]
fn a_record_without_an_account_id_cannot_synthesise_a_restorable_entry() {
    let fixture = Fixture::new();
    let tokens = TokenSet {
        access_token: "opaque-token".to_string(),
        refresh_token: Some("rt-1".to_string()),
        expires_at: Some(1_800_000_000),
        ..TokenSet::default()
    };
    // 没有 user_id 的条目在 CLI 里表现为"已登录但不可用"，宁可拒绝。
    let error = write(&fixture.roots, &request(&tokens, "dev@example.com"))
        .expect_err("a half-formed login is refused");
    assert!(error.to_string().contains("user_id"), "{error}");
    assert!(!fixture.auth_path().exists(), "nothing was written");
}

#[test]
fn a_switch_to_another_account_drops_the_previous_mirrored_claims() {
    let fixture = Fixture::new();
    let first = tokens("user-1", "first@example.com");
    write(&fixture.roots, &request(&first, "first@example.com")).expect("first");
    // CLI 自己补的团队字段：属于上一个账号。
    let path = fixture.auth_path();
    let mut body: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    body[grok::scope_key()]["team_name"] = json!("Previous Team");
    body[grok::scope_key()]["team_role"] = json!("owner");
    std::fs::write(&path, serde_json::to_vec(&body).expect("json")).expect("write");

    let second = tokens("user-2", "second@example.com");
    write(&fixture.roots, &request(&second, "second@example.com")).expect("second");

    let after: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    let entry = &after[grok::scope_key()];
    assert_eq!(entry["email"], "second@example.com");
    assert_eq!(entry["user_id"], "user-2");
    assert!(entry.get("team_name").is_none(), "the previous team survived: {entry}");
    assert!(entry.get("team_role").is_none());
}

#[test]
fn other_scope_entries_and_unknown_keys_survive() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "https://auth.x.ai::some-other-client": { "key": "other-token" },
            "global_hint": "keep-me"
        }))
        .expect("json"),
    )
    .expect("seed");

    let tokens = tokens("user-1", "dev@example.com");
    write(&fixture.roots, &request(&tokens, "dev@example.com")).expect("write");

    let body: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(body["https://auth.x.ai::some-other-client"]["key"], "other-token");
    assert_eq!(body["global_hint"], "keep-me");
    assert_eq!(body[grok::scope_key()]["key"], tokens.access_token);
}

#[test]
fn reads_back_the_login_and_its_identity() {
    let fixture = Fixture::new();
    let tokens = tokens("user-1", "dev@example.com");
    write(&fixture.roots, &request(&tokens, "dev@example.com")).expect("write");

    let local = read(&fixture.roots).expect("read").expect("login");
    assert_eq!(local.tokens.access_token, tokens.access_token);
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-1"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.account_id.as_deref(), Some("user-1"));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));

    let live = switch::live_with(&fixture.roots, "grok").expect("live");
    assert_eq!(live.identity.as_deref(), Some("user-1"));
    assert_eq!(
        live.access_fingerprint,
        Some(crate::identity::fingerprint(&tokens.access_token))
    );
}

#[test]
fn a_file_without_any_key_is_not_a_login() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"https://auth.x.ai::x":{"refresh_token":"rt"}}"#).expect("seed");
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn the_snapshot_puts_the_previous_file_back() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(&path, r#"{"https://auth.x.ai::old":{"key":"previous"}}"#).expect("seed");

    let snapshot = snapshot(&fixture.roots).expect("snapshot");
    let tokens = tokens("user-1", "dev@example.com");
    write(&fixture.roots, &request(&tokens, "dev@example.com")).expect("write");
    switch::restore(&snapshot).expect("restore");

    let restored: Value = serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
    assert_eq!(restored["https://auth.x.ai::old"]["key"], "previous");
    assert!(restored.get(grok::scope_key()).is_none());
}

#[test]
fn live_identity_prefers_the_provider_account_id() {
    let both = crate::oauth::Identity {
        email: Some("dev@example.com".to_string()),
        account_id: Some("user-1".to_string()),
        ..crate::oauth::Identity::default()
    };
    assert_eq!(live_identity(&both), Some("user-1".to_string()));
    let email_only = crate::oauth::Identity {
        email: Some("dev@example.com".to_string()),
        ..crate::oauth::Identity::default()
    };
    assert_eq!(live_identity(&email_only).as_deref(), Some("dev@example.com"));
    assert_eq!(live_identity(&crate::oauth::Identity::default()), None);
}

#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    let tokens = tokens("user-1", "dev@example.com");
    let outcome = write(&roots, &request(&tokens, "dev@example.com")).expect("write");
    assert!(
        PathBuf::from(&outcome.target).starts_with(sandbox.root()),
        "{} escaped the sandbox",
        outcome.target
    );
}
