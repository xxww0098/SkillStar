//! Claude Code credential-file tests: the segment we own, the keys we must not
//! touch, the millisecond expiry, and the rollback material.

use std::path::{Path, PathBuf};

use crate::oauth::TokenSet;

use super::{credential_path, read, snapshot, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::Sandbox;

struct Fixture {
    /// Held only so the temp directory outlives every path resolved from it.
    _dir: tempfile::TempDir,
    roots: PathRoots,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        Self { _dir: dir, roots }
    }

    fn credentials_path(&self) -> PathBuf {
        credential_path(&self.roots)
    }

    /// Seed the file Claude Code would already have on disk.
    fn seed(&self, body: &[u8]) {
        let path = self.credentials_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
    }
}

fn tokens(access: &str, refresh: Option<&str>, expires_at: Option<i64>) -> TokenSet {
    TokenSet {
        access_token: access.to_string(),
        refresh_token: refresh.map(str::to_string),
        id_token: None,
        expires_at,
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "claude-code",
        tokens,
        provider_state: None,
        display_name: "dev@example.com",
        account_id: None,
        email: Some("dev@example.com"),
    }
}

#[test]
fn writes_the_segment_and_keeps_every_other_key() {
    let fixture = Fixture::new();
    fixture.seed(
        br#"{"claudeAiOauth":{"accessToken":"old","refreshToken":"old-rt","expiresAt":1,
            "scopes":["user:inference"],"subscriptionType":"max"},"mcpOAuth":{"server":{"token":"keep"}}}"#,
    );

    let tokens = tokens("new-access", Some("new-refresh"), Some(1_800_000_000));
    let outcome = write(&fixture.roots, &request(&tokens)).expect("write");
    assert_eq!(outcome.agent, "claude-code");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.credentials_path()).unwrap()).unwrap();
    assert_eq!(body["claudeAiOauth"]["accessToken"], "new-access");
    assert_eq!(body["claudeAiOauth"]["refreshToken"], "new-refresh");
    // Milliseconds in this file; the OAuth module counts seconds.
    assert_eq!(body["claudeAiOauth"]["expiresAt"], 1_800_000_000_000_i64);
    // Everything we do not own survives untouched.
    assert_eq!(body["claudeAiOauth"]["scopes"][0], "user:inference");
    assert_eq!(body["claudeAiOauth"]["subscriptionType"], "max");
    assert_eq!(body["mcpOAuth"]["server"]["token"], "keep");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(fixture.credentials_path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn a_session_we_cannot_complete_drops_the_previous_account_s_leftovers() {
    let fixture = Fixture::new();
    fixture.seed(
        br#"{"claudeAiOauth":{"accessToken":"old","refreshToken":"old-rt","expiresAt":1}}"#,
    );

    // No refresh token and no known expiry: writing the previous account's
    // values back would hand Claude Code that account's session.
    let tokens = tokens("access-only", None, None);
    write(&fixture.roots, &request(&tokens)).expect("write");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.credentials_path()).unwrap()).unwrap();
    assert_eq!(body["claudeAiOauth"]["accessToken"], "access-only");
    assert!(body["claudeAiOauth"].get("refreshToken").is_none());
    assert!(body["claudeAiOauth"].get("expiresAt").is_none());
}

#[test]
fn live_reports_the_email_the_segment_carries() {
    let fixture = Fixture::new();
    fixture.seed(
        br#"{"claudeAiOauth":{"accessToken":"at","refreshToken":"rt","expiresAt":2,"email":"dev@example.com"}}"#,
    );

    let live = switch::live_with(&fixture.roots, "claude-code").expect("live");
    assert_eq!(live.identity.as_deref(), Some("dev@example.com"));
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("at").as_str())
    );
    assert!(live.source.ends_with(".credentials.json"));
}

#[test]
fn a_written_segment_is_matched_by_its_fingerprint() {
    let fixture = Fixture::new();
    let tokens = tokens("fresh-access", Some("fresh-refresh"), Some(1_800_000_000));
    write(&fixture.roots, &request(&tokens)).expect("write");

    // The segment carries no email, so the switch is verified by the token it
    // wrote, not by an identity Claude Code never stored.
    let live = switch::live_with(&fixture.roots, "claude-code").expect("live");
    assert_eq!(live.identity, None);
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("fresh-access").as_str())
    );
    assert_eq!(
        live.refresh_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("fresh-refresh").as_str())
    );
}

#[test]
fn read_extracts_subscription_or_rate_limit_tier_into_provider_state_plan() {
    let fixture = Fixture::new();
    fixture.seed(
        br#"{"claudeAiOauth":{"accessToken":"at-max","refreshToken":"rt-max","subscriptionType":"max","rateLimitTier":"default_claude_max_5x"}}"#,
    );
    let local = read(&fixture.roots).expect("read").expect("credential");
    assert_eq!(local.identity.plan.as_deref(), Some("max"));
    assert_eq!(
        local.provider_state,
        Some(serde_json::json!({ "plan": "max" }))
    );

    let fallback = Fixture::new();
    fallback.seed(
        br#"{"claudeAiOauth":{"accessToken":"at-tier","rateLimitTier":"default_claude_pro"}}"#,
    );
    let local_fallback = read(&fallback.roots).expect("read").expect("credential");
    assert_eq!(
        local_fallback.identity.plan.as_deref(),
        Some("default_claude_pro")
    );
    assert_eq!(
        local_fallback.provider_state,
        Some(serde_json::json!({ "plan": "default_claude_pro" }))
    );
}

#[test]
fn read_imports_the_session_and_converts_the_expiry() {
    let fixture = Fixture::new();
    fixture.seed(
        br#"{"claudeAiOauth":{"accessToken":"at-import","refreshToken":"rt-import","expiresAt":1799999999000,"email":"dev@example.com"}}"#,
    );

    let local = read(&fixture.roots).expect("read").expect("a session");
    assert_eq!(local.tokens.access_token, "at-import");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-import"));
    assert_eq!(local.tokens.expires_at, Some(1_799_999_999));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
}

#[test]
fn a_file_without_the_segment_has_nothing_to_import() {
    let fixture = Fixture::new();
    fixture.seed(br#"{"mcpOAuth":{"server":{}}}"#);
    assert!(read(&fixture.roots).expect("read").is_none());

    let empty = Fixture::new();
    assert!(read(&empty.roots).expect("read").is_none());
}

#[test]
fn a_corrupt_file_is_reported_instead_of_overwritten() {
    let fixture = Fixture::new();
    fixture.seed(b"this is not json");
    let tokens = tokens("at", Some("rt"), None);
    let error = write(&fixture.roots, &request(&tokens)).expect_err("corrupt file");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert_eq!(std::fs::read(fixture.credentials_path()).unwrap(), b"this is not json");
}

#[test]
fn snapshot_puts_the_previous_credentials_back() {
    let fixture = Fixture::new();
    fixture.seed(br#"{"claudeAiOauth":{"accessToken":"previous"}}"#);
    let captured = snapshot(&fixture.roots).expect("snapshot");

    let tokens = tokens("switched", Some("rt"), Some(1_800_000_000));
    write(&fixture.roots, &request(&tokens)).expect("write");
    switch::restore(&captured).expect("restore");

    let body: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.credentials_path()).unwrap()).unwrap();
    assert_eq!(body["claudeAiOauth"]["accessToken"], "previous");
    assert!(body["claudeAiOauth"].get("refreshToken").is_none());
}

#[test]
fn the_path_is_the_config_directory_the_cli_reads() {
    let dir = tempfile::tempdir().unwrap();
    let roots = PathRoots::for_os(DesktopOs::Linux, dir.path());
    assert_eq!(
        credential_path(&roots),
        Path::new(dir.path()).join(".claude/.credentials.json")
    );
}
#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    assert_eq!(
        credential_path(&roots),
        sandbox.tool_path(&[".claude", ".credentials.json"])
    );

    let tokens = tokens("at-env", Some("rt-env"), Some(1_800_000_000));
    switch::write(&request(&tokens)).expect("write through the environment");
    let written = sandbox.tool_path(&[".claude", ".credentials.json"]);
    assert!(written.is_file(), "{}", written.display());

    let live = switch::live("claude-code").expect("live through the environment");
    assert_eq!(live.source, written.display().to_string());
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("at-env").as_str())
    );
}