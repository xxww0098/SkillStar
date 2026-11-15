//! ZCode 切号测试：enc:v1 回读、两处文件、与客户端格式的一致性。
//!
//! 路径与密钥都经注入：PathRoots 指向 temp，密钥按 temp 的 home 派生。

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::oauth::TokenSet;

use super::{credential_path, live_identity, read, settings_path, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::{Sandbox, jwt_with_claims};

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

    fn credentials(&self) -> PathBuf {
        credential_path(&self.roots)
    }
}

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: format!("at-{marker}"),
        refresh_token: Some(format!("rt-{marker}")),
        id_token: Some(jwt_with_claims(json!({
            "email": "dev@example.com",
            "sub": "user-1",
            "exp": 1_800_000_000_i64
        }))),
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet, provider: &'a Value) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "zcode",
        tokens,
        provider_state: Some(provider),
        display_name: "dev@example.com",
        account_id: Some("user-1"),
        email: Some("dev@example.com"),
    }
}

fn zai() -> Value {
    json!({ "provider": "zai" })
}

#[test]
fn a_switch_writes_sealed_values_and_the_family_mode() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens, &zai())).expect("write");
    assert_eq!(outcome.agent, "zcode");
    assert_eq!(outcome.live.as_deref(), Some("user-1"));
    assert_eq!(
        outcome.target,
        fixture.home().join("home/.zcode/v2/credentials.json").display().to_string()
    );

    let body: Value =
        serde_json::from_slice(&std::fs::read(fixture.credentials()).expect("read")).expect("json");
    for key in [
        "oauth:active_provider",
        "oauth:zai:access_token",
        "oauth:zai:refresh_token",
        "oauth:zai:user_info",
        "zcodejwttoken",
    ] {
        let value = body.get(key).and_then(Value::as_str).unwrap_or_default();
        assert!(value.starts_with("enc:v1:"), "{key} is not sealed: {value}");
        assert!(!value.contains("at-one"), "{key} leaks the token");
    }

    let settings: Value = serde_json::from_slice(
        &std::fs::read(settings_path(&fixture.roots)).expect("setting"),
    )
    .expect("json");
    assert_eq!(settings["modelProviderFamilyModes"]["zai"], "oauth");
}

#[test]
fn a_second_switch_replaces_only_its_own_provider_keys() {
    let fixture = Fixture::new();
    let first = tokens("one");
    write(&fixture.roots, &request(&first, &zai())).expect("first");
    let second = tokens("two");
    write(&fixture.roots, &request(&second, &json!({ "provider": "bigmodel" })))
        .expect("second");

    let body: Value =
        serde_json::from_slice(&std::fs::read(fixture.credentials()).expect("read")).expect("json");
    assert!(body.get("oauth:zai:access_token").is_some(), "the other provider's row stays");
    assert!(body.get("oauth:bigmodel:access_token").is_some());
    let settings: Value = serde_json::from_slice(
        &std::fs::read(settings_path(&fixture.roots)).expect("setting"),
    )
    .expect("json");
    assert_eq!(settings["modelProviderFamilyModes"]["bigmodel"], "oauth");
}

#[test]
fn a_switch_without_a_jwt_is_refused_before_any_write() {
    let fixture = Fixture::new();
    let mut tokens = tokens("one");
    tokens.id_token = None;
    let error = write(&fixture.roots, &request(&tokens, &zai())).expect_err("no jwt");
    assert!(error.to_string().contains("JWT"), "{error}");
    assert!(!fixture.credentials().exists());
}

#[test]
fn reads_back_the_session_through_the_same_key() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, &zai())).expect("write");

    let local = read(&fixture.roots).expect("read").expect("session");
    assert_eq!(local.tokens.access_token, "at-one");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-one"));
    assert!(local.tokens.id_token.is_some(), "the jwt comes back as the id token");
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.identity.account_id.as_deref(), Some("user-1"));

    let live = switch::live_with(&fixture.roots, "zcode").expect("live");
    assert_eq!(live.identity.as_deref(), Some("user-1"));
}

#[test]
fn a_plaintext_value_is_reported_instead_of_read_as_a_session() {
    let fixture = Fixture::new();
    let path = fixture.credentials();
    std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({ "oauth:active_provider": "zai" })).expect("json"),
    )
    .expect("seed");
    let error = read(&fixture.roots).expect_err("a plaintext provider is not a session");
    assert!(error.to_string().contains("enc:v1"), "{error}");
}

#[test]
fn the_snapshot_puts_both_files_back() {
    let fixture = Fixture::new();
    let credentials = fixture.credentials();
    let settings = settings_path(&fixture.roots);
    std::fs::create_dir_all(credentials.parent().expect("dir")).expect("dir");
    std::fs::write(&credentials, r#"{"keep":"me"}"#).expect("seed");
    std::fs::write(&settings, r#"{"dataBaseDir":"/tmp/other"}"#).expect("seed settings");

    let snapshot = switch::snapshot_with(&fixture.roots, "zcode").expect("snapshot");
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens, &zai())).expect("write");
    switch::restore(&snapshot).expect("restore");

    let restored: Value =
        serde_json::from_slice(&std::fs::read(&credentials).expect("read")).expect("json");
    assert_eq!(restored["keep"], "me");
    let restored_settings: Value =
        serde_json::from_slice(&std::fs::read(&settings).expect("read")).expect("json");
    assert_eq!(restored_settings["dataBaseDir"], "/tmp/other");
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
    let outcome = write(&roots, &request(&tokens, &zai())).expect("write");
    assert!(
        PathBuf::from(&outcome.target).starts_with(sandbox.root()),
        "{} escaped the sandbox",
        outcome.target
    );
}
