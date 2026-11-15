//! Codex auth-file tests: the shape the CLI reads, the read-back, and the
//! rollback material. Every write goes into a temp directory: the paths are
//! injected, so no test can reach a real ~/.codex.

use std::path::{Path, PathBuf};

use crate::oauth::TokenSet;

use super::{credential_path, live_identity, read, snapshot, write};
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

fn id_token(account_id: &str) -> String {
    jwt_with_claims(serde_json::json!({
        "email": "dev@example.com",
        "https://api.openai.com/auth": { "chatgpt_account_id": account_id }
    }))
}

/// Codex's auth.json carries no expiry of its own: the access token's own exp
/// claim is the only clock, which is what the reader has to fall back to.
fn access_token(marker: &str) -> String {
    jwt_with_claims(serde_json::json!({ "sub": format!("acc-{marker}"), "exp": 1_800_000_000_i64 }))
}

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: access_token(marker),
        refresh_token: Some(format!("rt-{marker}")),
        id_token: Some(id_token(marker)),
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "codex",
        tokens,
        provider_state: None,
        display_name: "dev@example.com",
        account_id: None,
        email: Some("dev@example.com"),
    }
}

#[test]
fn writes_the_shape_the_cli_reads() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens)).expect("write");
    assert_eq!(outcome.agent, "codex");
    assert_eq!(outcome.live.as_deref(), Some("one"));
    assert_eq!(
        outcome.target,
        fixture.home().join("home/.codex/auth.json").display().to_string()
    );

    let path = fixture.auth_path();
    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("auth.json")).expect("json");
    assert!(body["OPENAI_API_KEY"].is_null());
    assert_eq!(body["tokens"]["access_token"], access_token("one"));
    assert_eq!(body["tokens"]["refresh_token"], "rt-one");
    assert_eq!(body["tokens"]["id_token"], tokens.id_token.clone().unwrap());
    // The account id comes from the record when it has one, else from the id
    // token the provider signed.
    assert_eq!(body["tokens"]["account_id"], "one");
    let last_refresh = body["last_refresh"].as_str().expect("last_refresh");
    assert!(last_refresh.ends_with('Z'), "{last_refresh}");
    assert!(
        chrono::DateTime::parse_from_rfc3339(last_refresh).is_ok(),
        "{last_refresh} is not RFC 3339"
    );
    assert_eq!(
        last_refresh.split('.').nth(1).map(str::len),
        Some(7),
        "microseconds plus the Z"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn keeps_the_keys_the_cli_itself_wrote() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        br#"{"cli_state":{"window":"main"},"tokens":{"access_token":"old","extra":"kept"}}"#,
    )
    .unwrap();

    write(&fixture.roots, &request(&tokens("two"))).expect("write");

    let body: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(body["cli_state"]["window"], "main");
    assert_eq!(body["tokens"]["extra"], "kept");
    assert_eq!(body["tokens"]["access_token"], access_token("two"));
}

#[test]
fn live_and_read_agree_on_the_account_id() {
    let fixture = Fixture::new();
    assert_eq!(live_identity(&Default::default()), None);

    let tokens = tokens("three");
    write(&fixture.roots, &request(&tokens)).expect("write");

    let live = switch::live_with(&fixture.roots, "codex").expect("live");
    assert_eq!(live.identity.as_deref(), Some("three"));
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint(&access_token("three")).as_str())
    );
    assert_eq!(
        live.refresh_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("rt-three").as_str())
    );
    assert!(live.source.ends_with(".codex/auth.json"));

    let local = read(&fixture.roots).expect("read").expect("a session");
    assert_eq!(local.tokens.access_token, access_token("three"));
    assert_eq!(local.identity.account_id.as_deref(), Some("three"));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
}

#[test]
fn an_api_key_only_file_has_no_session_to_import() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, br#"{"OPENAI_API_KEY":"sk-api-key","tokens":null}"#).unwrap();

    assert!(read(&fixture.roots).expect("read").is_none());
    let live = switch::live_with(&fixture.roots, "codex").expect("live");
    assert_eq!(live.identity, None);
}

#[test]
fn a_missing_file_is_an_empty_live_credential_not_an_error() {
    let fixture = Fixture::new();
    let live = switch::live_with(&fixture.roots, "codex").expect("live");
    assert_eq!(live.identity, None);
    assert_eq!(live.access_fingerprint, None);
    assert!(live.source.ends_with("auth.json"));
}

#[test]
fn snapshot_puts_the_previous_auth_file_back() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let previous = br#"{"OPENAI_API_KEY":null,"tokens":{"access_token":"previous"}}"#;
    std::fs::write(&path, previous).unwrap();

    let captured = snapshot(&fixture.roots).expect("snapshot");
    write(&fixture.roots, &request(&tokens("switched"))).expect("write");
    assert_ne!(std::fs::read(&path).unwrap(), previous.to_vec());

    switch::restore(&captured).expect("restore");
    assert_eq!(std::fs::read(&path).unwrap(), previous.to_vec());
}

#[test]
fn snapshot_restores_absence_when_there_was_no_auth_file() {
    let fixture = Fixture::new();
    let path = fixture.auth_path();

    let captured = snapshot(&fixture.roots).expect("snapshot");
    write(&fixture.roots, &request(&tokens("first"))).expect("write");
    assert!(path.is_file());

    switch::restore(&captured).expect("restore");
    assert!(!path.exists(), "a client that had no credential gets none back");
}

#[test]
fn a_record_account_id_wins_over_the_token_claim() {
    let fixture = Fixture::new();
    let tokens = tokens("claimant");
    let mut request = request(&tokens);
    request.account_id = Some("record-account");
    write(&fixture.roots, &request).expect("write");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.auth_path()).unwrap()).unwrap();
    assert_eq!(body["tokens"]["account_id"], "record-account");
}

#[test]
fn a_tokenset_without_an_access_token_is_refused() {
    let fixture = Fixture::new();
    let mut tokens = tokens("empty");
    tokens.access_token = "   ".to_string();
    let error = write(&fixture.roots, &request(&tokens)).expect_err("no access token");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert!(!fixture.auth_path().exists());
}
#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    // SKILLSTAR_TOOL_SYNC_HOME is what stops the public entry points (the ones
    // that read the process environment) from touching a developer's ~/.codex.
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    assert_eq!(
        roots.sandbox_home.as_deref(),
        Some(sandbox.tool_path(&[]).as_path())
    );
    assert_eq!(
        credential_path(&roots),
        sandbox.tool_path(&[".codex", "auth.json"])
    );

    let tokens = tokens("env");
    switch::write(&request(&tokens)).expect("write through the environment");
    let written = sandbox.tool_path(&[".codex", "auth.json"]);
    assert!(written.is_file(), "{}", written.display());

    let live = switch::live("codex").expect("live through the environment");
    assert_eq!(live.identity.as_deref(), Some("env"));
    assert_eq!(live.source, written.display().to_string());

    let local = read(&roots).expect("read").expect("a session");
    assert_eq!(local.tokens.access_token, access_token("env"));
}
