//! Cursor state.vscdb tests: the rows a switch writes, the read-back, the
//! rollback material, and the database that refuses to keep a session.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use crate::oauth::TokenSet;

use super::{credential_path, read, snapshot, write};
use crate::switch::{self, DesktopOs, PathRoots, SwitchRequest};
use crate::test_support::jwt_with_claims;

/// A temp Cursor install: its user-data root and the roots that resolve into it.
struct Fixture {
    dir: tempfile::TempDir,
    roots: PathRoots,
    db: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        // The sandbox root is what keeps a real ~/Library out of reach.
        roots.sandbox_home = Some(dir.path().join("tool"));
        let db = credential_path(&roots);
        Self { dir, roots, db }
    }

    /// The database Cursor itself creates on first launch.
    fn with_database() -> Self {
        let fixture = Self::new();
        std::fs::create_dir_all(fixture.db.parent().unwrap()).unwrap();
        fixture
            .connection()
            .execute_batch(
                "CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);",
            )
            .unwrap();
        fixture
    }

    fn home(&self) -> &Path {
        self.dir.path()
    }

    fn connection(&self) -> Connection {
        Connection::open(&self.db).expect("state.vscdb")
    }

    fn value(&self, key: &str) -> Option<String> {
        self.connection()
            .query_row("SELECT value FROM ItemTable WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .ok()
    }
}

fn access_token(auth_id: &str) -> String {
    jwt_with_claims(serde_json::json!({
        "sub": format!("user_01|{auth_id}"),
        "exp": 1_800_000_000_i64
    }))
}

fn tokens(auth_id: &str) -> TokenSet {
    TokenSet {
        access_token: access_token(auth_id),
        refresh_token: Some(format!("rt-{auth_id}")),
        id_token: None,
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "cursor",
        tokens,
        provider_state: None,
        display_name: "dev@example.com",
        account_id: None,
        email: Some("dev@example.com"),
    }
}

#[test]
fn writes_the_session_rows_and_the_mirrors() {
    let fixture = Fixture::with_database();
    let tokens = tokens("auth-9");
    let outcome = write(&fixture.roots, &request(&tokens)).expect("write");

    assert_eq!(outcome.agent, "cursor");
    assert_eq!(outcome.target, fixture.db.display().to_string());
    assert_eq!(fixture.value("cursorAuth/accessToken").as_deref(), Some(tokens.access_token.as_str()));
    assert_eq!(fixture.value("cursorAuth/refreshToken").as_deref(), Some("rt-auth-9"));
    assert_eq!(fixture.value("cursorAuth/cachedEmail").as_deref(), Some("dev@example.com"));
    // Cursor's subject is "<user>|<auth id>"; only the tail is the account id.
    assert_eq!(fixture.value("cursorAuth/authId").as_deref(), Some("auth-9"));
    assert_eq!(
        fixture.value("cursor.accessToken").as_deref(),
        Some(tokens.access_token.as_str())
    );
    assert_eq!(fixture.value("cursor.email").as_deref(), Some("dev@example.com"));
    assert!(
        fixture
            .home()
            .join("tool")
            .join("Library")
            .join("Application Support")
            .join("Cursor")
            .join("User")
            .join("globalStorage")
            .join("state.vscdb")
            .is_file(),
        "the sandboxed user-data path is the one written"
    );
}

#[test]
fn a_recorded_account_id_wins_over_the_token_subject() {
    let fixture = Fixture::with_database();
    let tokens = tokens("from-token");
    let mut request = request(&tokens);
    request.account_id = Some("from-record");
    write(&fixture.roots, &request).expect("write");
    assert_eq!(fixture.value("cursorAuth/authId").as_deref(), Some("from-record"));
}

#[test]
fn live_reports_the_cached_email() {
    let fixture = Fixture::with_database();
    let tokens = tokens("auth-1");
    write(&fixture.roots, &request(&tokens)).expect("write");

    let live = switch::live_with(&fixture.roots, "cursor").expect("live");
    assert_eq!(live.identity.as_deref(), Some("dev@example.com"));
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint(&tokens.access_token).as_str())
    );
    assert_eq!(live.refresh_fingerprint.as_deref(), Some(crate::identity::fingerprint("rt-auth-1").as_str()));
}

#[test]
fn read_imports_the_session() {
    let fixture = Fixture::with_database();
    let tokens = tokens("auth-2");
    write(&fixture.roots, &request(&tokens)).expect("write");

    let local = read(&fixture.roots).expect("read").expect("a session");
    assert_eq!(local.tokens.access_token, tokens.access_token);
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-auth-2"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.identity.account_id.as_deref(), Some("auth-2"));
}

#[test]
fn a_missing_database_is_refused_and_never_created() {
    let fixture = Fixture::new();
    let tokens = tokens("auth-3");
    let error = write(&fixture.roots, &request(&tokens)).expect_err("no database");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert!(!fixture.db.exists());
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn snapshot_puts_the_previous_session_back() {
    let fixture = Fixture::with_database();
    let first = tokens("auth-a");
    write(&fixture.roots, &request(&first)).expect("write");

    let captured = snapshot(&fixture.roots).expect("snapshot");
    let second = tokens("auth-b");
    write(&fixture.roots, &request(&second)).expect("write");
    assert_eq!(fixture.value("cursorAuth/authId").as_deref(), Some("auth-b"));

    switch::restore(&captured).expect("restore");
    assert_eq!(fixture.value("cursorAuth/authId").as_deref(), Some("auth-a"));
    assert_eq!(
        fixture.value("cursorAuth/accessToken").as_deref(),
        Some(first.access_token.as_str())
    );
}

#[test]
fn restoring_rows_that_never_existed_clears_them_again() {
    let fixture = Fixture::with_database();
    let captured = snapshot(&fixture.roots).expect("snapshot");
    assert_eq!(captured_rows(&captured).iter().filter(|row| row.1).count(), 0);

    let tokens = tokens("auth-temp");
    write(&fixture.roots, &request(&tokens)).expect("write");
    assert!(fixture.value("cursorAuth/accessToken").is_some());

    switch::restore(&captured).expect("restore");
    for key in [
        "cursorAuth/accessToken",
        "cursorAuth/refreshToken",
        "cursorAuth/cachedEmail",
        "cursorAuth/authId",
        "cursor.accessToken",
        "cursor.email",
    ] {
        assert_eq!(fixture.value(key), None, "{key} should have been cleared");
    }
    assert!(read(&fixture.roots).expect("read").is_none());
}

fn captured_rows(snapshot: &crate::switch::ClientSnapshot) -> Vec<(String, bool)> {
    match snapshot {
        crate::switch::ClientSnapshot::Items { rows, .. } => rows
            .iter()
            .map(|(key, value)| (key.clone(), value.is_some()))
            .collect(),
        other => panic!("cursor snapshots are ItemTable rows, got {other:?}"),
    }
}

#[test]
fn a_database_that_rejects_the_rows_fails_the_switch_and_keeps_the_old_session() {
    let fixture = Fixture::with_database();
    let previous = tokens("auth-old");
    write(&fixture.roots, &request(&previous)).expect("write");

    // Cursor (or another process) refuses a session that differs from the one
    // it holds: the write must fail, and the rows must still be the old ones.
    fixture
        .connection()
        .execute_batch(
            "CREATE TRIGGER reject_auth BEFORE INSERT ON ItemTable
             WHEN NEW.key = 'cursorAuth/accessToken'
              AND NEW.value <> (SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken')
             BEGIN SELECT RAISE(ABORT, 'fixture rejects a different session'); END;",
        )
        .unwrap();

    let next = tokens("auth-new");
    let error = write(&fixture.roots, &request(&next)).expect_err("the trigger refuses");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert_eq!(
        fixture.value("cursorAuth/accessToken").as_deref(),
        Some(previous.access_token.as_str())
    );
    assert_eq!(fixture.value("cursorAuth/authId").as_deref(), Some("auth-old"));
}

#[test]
fn a_tokenset_without_an_access_token_is_refused() {
    let fixture = Fixture::with_database();
    let mut tokens = tokens("auth-empty");
    tokens.access_token = "  ".to_string();
    let error = write(&fixture.roots, &request(&tokens)).expect_err("no access token");
    assert!(matches!(error, crate::AccountError::Client(_)));
    assert_eq!(fixture.value("cursorAuth/accessToken"), None);
}

#[test]
fn the_path_follows_the_platform_user_data_root() {
    let dir = tempfile::tempdir().unwrap();
    let macos = PathRoots::for_os(DesktopOs::Macos, dir.path());
    assert_eq!(
        credential_path(&macos),
        Path::new(dir.path())
            .join("Library/Application Support/Cursor/User/globalStorage/state.vscdb")
    );
}
#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let _sandbox = crate::test_support::Sandbox::new();
    let path = credential_path(&PathRoots::from_env());
    assert!(
        path.starts_with(_sandbox.tool_path(&[])),
        "{} escaped the sandbox",
        path.display()
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);")
        .unwrap();

    let tokens = tokens("auth-env");
    switch::write(&request(&tokens)).expect("write through the environment");
    let live = switch::live("cursor").expect("live through the environment");
    assert_eq!(live.identity.as_deref(), Some("dev@example.com"));
    assert_eq!(live.source, path.display().to_string());
}

#[test]
fn encrypted_primary_tokens_fall_back_to_plaintext_mirror_and_reject_missing_mirror() {
    let fixture = Fixture::with_database();
    let jwt = access_token("auth-enc");
    {
        let conn = fixture.connection();
        for (key, value) in [
            ("cursorAuth/accessToken", "cursor-safe-storage:v1:encryptedAt"),
            ("cursorAuth/refreshToken", "cursor-safe-storage:v1:encryptedRt"),
            ("cursorAuth/authId", "auth-enc"),
            ("cursor.accessToken", jwt.as_str()),
            ("cursor.email", "mirror@example.com"),
        ] {
            conn.execute(
                "INSERT OR REPLACE INTO ItemTable (key, value) VALUES (?1, ?2)",
                [key, value],
            )
            .expect("seed");
        }
    }

    let local = read(&fixture.roots).expect("read").expect("session");
    assert_eq!(local.tokens.access_token, jwt);
    assert_eq!(local.tokens.refresh_token, None);
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("mirror@example.com"));
    assert_eq!(local.identity.account_id.as_deref(), Some("auth-enc"));

    let live = switch::live_with(&fixture.roots, "cursor").expect("live");
    assert_eq!(live.identity.as_deref(), Some("mirror@example.com"));

    // 缺少明文镜像时给出明确错误而非把密文当成有效凭据返回。
    fixture
        .connection()
        .execute("DELETE FROM ItemTable WHERE key = 'cursor.accessToken'", [])
        .expect("drop mirror");
    let missing_err = read(&fixture.roots).expect_err("encrypted without mirror must error");
    assert!(matches!(missing_err, crate::AccountError::Client(_)));

    // write 也禁止把密文写回客户端。
    let bad_tokens = TokenSet {
        access_token: "cursor-safe-storage:v1:bad".to_string(),
        ..TokenSet::default()
    };
    let write_err = write(&fixture.roots, &request(&bad_tokens)).expect_err("refuse ciphertext write");
    assert!(matches!(write_err, crate::AccountError::Client(_)));
}