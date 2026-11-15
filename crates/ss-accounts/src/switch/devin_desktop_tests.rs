//! Devin Desktop 切号测试：三行状态、回读、导入与回滚。
//!
//! 数据库在 temp 目录里现造一张 ItemTable，路径经 PathRoots 注入。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{Value, json};

use crate::oauth::TokenSet;

use super::{AUTH_STATUS_KEY, credential_path, live_identity, read, snapshot, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::Sandbox;

struct Fixture {
    // 目录只在测试期间存在：靠它保证 temp 生命周期覆盖 roots 里的路径。
    _dir: tempfile::TempDir,
    roots: PathRoots,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        Self { _dir: dir, roots }
    }

    /// 造一张和客户端同形的库，并预置一行"上一个账号"。
    fn with_database(&self) -> PathBuf {
        let path = credential_path(&self.roots);
        std::fs::create_dir_all(path.parent().expect("dir")).expect("dir");
        let connection = Connection::open(&path).expect("db");
        connection
            .execute(
                "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT)",
                [],
            )
            .expect("table");
        connection
            .execute(
                "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                [
                    AUTH_STATUS_KEY,
                    r#"{"apiKey":"previous-key","email":"previous@example.com"}"#,
                ],
            )
            .expect("seed");
        path
    }
}

fn tokens(marker: &str) -> TokenSet {
    TokenSet {
        access_token: format!("key-{marker}"),
        refresh_token: None,
        id_token: None,
        expires_at: None,
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "devin-desktop",
        tokens,
        provider_state: None,
        display_name: "dev@example.com",
        account_id: None,
        email: Some("dev@example.com"),
    }
}

fn row(path: &Path, key: &str) -> Option<String> {
    let connection = Connection::open(path).expect("db");
    connection
        .query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |cursor| {
            cursor.get::<_, Option<String>>(0)
        })
        .ok()
        .flatten()
}

#[test]
fn a_switch_writes_the_three_rows_the_client_reads() {
    let fixture = Fixture::new();
    let path = fixture.with_database();
    let tokens = tokens("one");
    let outcome = write(&fixture.roots, &request(&tokens)).expect("write");
    assert_eq!(outcome.agent, "devin-desktop");
    assert_eq!(outcome.live.as_deref(), Some("dev@example.com"));

    let status: Value =
        serde_json::from_str(&row(&path, AUTH_STATUS_KEY).expect("status")).expect("json");
    assert_eq!(status["apiKey"], "key-one");
    assert_eq!(status["email"], "dev@example.com");
    let selected = row(&path, "codeium.windsurf-windsurf_auth").expect("selected");
    assert_eq!(
        selected,
        row(&path, AUTH_STATUS_KEY).expect("status"),
        "both rows carry one login"
    );
}

#[test]
fn a_missing_client_database_is_refused_instead_of_created() {
    let fixture = Fixture::new();
    let tokens = tokens("one");
    let error = write(&fixture.roots, &request(&tokens)).expect_err("no database");
    assert!(error.to_string().contains("state.vscdb"), "{error}");
    assert!(!credential_path(&fixture.roots).exists(), "no database is invented");
}

#[test]
fn reads_back_the_login_and_its_identity() {
    let fixture = Fixture::new();
    let path = fixture.with_database();
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens)).expect("write");

    let local = read(&fixture.roots).expect("read").expect("login");
    assert_eq!(local.tokens.access_token, "key-one");
    assert_eq!(local.tokens.refresh_token, None);
    assert_eq!(local.tokens.expires_at, None, "an apiKey has no expiry");
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert!(local.provider_state.is_none(), "no api server was known");

    let live = switch::live_with(&fixture.roots, "devin-desktop").expect("live");
    assert_eq!(live.identity.as_deref(), Some("dev@example.com"));
    assert_eq!(
        live.access_fingerprint,
        Some(crate::identity::fingerprint("key-one"))
    );
    assert!(path.is_file());
}

#[test]
fn the_api_server_row_is_written_and_imported() {
    let fixture = Fixture::new();
    fixture.with_database();
    let tokens = tokens("one");
    let state = json!({ "api_server_url": "https://server.self-serve.windsurf.com" });
    let request = SwitchRequest {
        provider_state: Some(&state),
        ..request(&tokens)
    };
    write(&fixture.roots, &request).expect("write");

    let local = read(&fixture.roots).expect("read").expect("login");
    assert_eq!(
        local
            .provider_state
            .as_ref()
            .and_then(|state| state.get("api_server_url")),
        Some(&json!("https://server.self-serve.windsurf.com"))
    );
}

#[test]
fn a_database_without_a_key_is_not_a_login() {
    let fixture = Fixture::new();
    let path = fixture.with_database();
    let connection = Connection::open(&path).expect("db");
    connection.execute("DELETE FROM ItemTable", []).expect("clear");
    connection
        .execute(
            "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
            [AUTH_STATUS_KEY, r#"{"email":"dev@example.com"}"#],
        )
        .expect("seed");
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn the_snapshot_puts_the_previous_rows_back() {
    let fixture = Fixture::new();
    let path = fixture.with_database();
    let snapshot = snapshot(&fixture.roots).expect("snapshot");
    let tokens = tokens("one");
    write(&fixture.roots, &request(&tokens)).expect("write");
    switch::restore(&snapshot).expect("restore");

    let status: Value =
        serde_json::from_str(&row(&path, AUTH_STATUS_KEY).expect("status")).expect("json");
    assert_eq!(status["apiKey"], "previous-key");
}

#[test]
fn live_identity_is_the_email_the_status_row_shows() {
    let identity = crate::oauth::Identity {
        email: Some("dev@example.com".to_string()),
        ..crate::oauth::Identity::default()
    };
    assert_eq!(live_identity(&identity).as_deref(), Some("dev@example.com"));
    assert_eq!(live_identity(&crate::oauth::Identity::default()), None);
}

#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = Sandbox::new();
    let roots = PathRoots::from_env();
    // 沙箱里没有库：写入必须拒绝。
    let tokens = tokens("one");
    let error = write(&roots, &request(&tokens)).expect_err("no database in the sandbox");
    let message = error.to_string();
    assert!(message.contains("state.vscdb"), "{message}");
    assert!(
        message.contains(&sandbox.root().display().to_string()),
        "the message must name a sandbox path: {message}"
    );
}
