//! Antigravity 切号测试：vscdb 投影、系统凭据、回退文件与整组回滚。
//!
//! Keychain 全程走注入的命令执行器：这套测试绝不该真的去动开发者的钥匙串。

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rusqlite::Connection;

use super::{
    OAUTH_TOKEN_KEY, credential_path, read, read_with_runner, snapshot_with_runner,
    write, write_with_runner,
};
use crate::oauth::TokenSet;
use crate::switch::{self, ClientSnapshot, DesktopOs, PathRoots, SwitchRequest};

/// 一个临时 Antigravity 安装：user-data 根、数据库与 roots。
struct Fixture {
    dir: tempfile::TempDir,
    roots: PathRoots,
    db: PathBuf,
}

impl Fixture {
    /// 沙箱 roots：只写 vscdb，Keychain 与真实 home 都碰不到。
    fn sandboxed() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        roots.sandbox_home = Some(dir.path().join("tool"));
        let db = credential_path(&roots);
        Self { dir, roots, db }
    }

    /// 真机 macOS 布局，但 home 是临时目录：系统凭据走注入的执行器，文件仍
    /// 落在临时目录里。
    fn macos() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let roots = PathRoots::for_os(DesktopOs::Macos, dir.path().join("home"));
        let db = credential_path(&roots);
        Self { dir, roots, db }
    }

    /// 真机 Windows 布局，但 home 是临时目录：WinCred 走注入的假存储，文件仍在临时目录里。
    fn windows() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let roots = PathRoots::for_os(DesktopOs::Windows, dir.path().join("home"));
        let db = credential_path(&roots);
        Self { dir, roots, db }
    }

    /// 客户端首次启动后留下的空库。
    fn with_database(self) -> Self {
        create_database(&self.db);
        self
    }

    fn home(&self) -> &Path {
        self.roots.home.as_path()
    }

    fn gemini(&self) -> PathBuf {
        crate::switch::paths::antigravity_gemini_dir(&self.roots)
    }

    /// 客户端自己装上 agy CLI 之后才有的目录。
    fn with_cli_session_dir(&self) -> PathBuf {
        let dir = self.gemini().join("antigravity-cli");
        std::fs::create_dir_all(&dir).expect("cli session dir");
        dir.join("antigravity-oauth-token")
    }

    fn connection(&self) -> Connection {
        Connection::open(&self.db).expect("state.vscdb")
    }

    fn row(&self) -> Option<String> {
        self.connection()
            .query_row(
                "SELECT value FROM ItemTable WHERE key = ?1",
                [OAUTH_TOKEN_KEY],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }
}

/// 直接写一行会话（不碰 Keychain / 回退文件），用于只关心数据库的用例。
fn write_row(fixture: &Fixture, access: &str, refresh: &str, email: Option<&str>) {
    super::write_oauth_row(
        &fixture.db,
        &super::Session {
            access_token: access,
            refresh_token: refresh,
            expires_at: 1_800_000_000,
            email,
            id_token: None,
        },
    )
    .expect("write the row");
}

fn create_database(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("db dir");
    Connection::open(path)
        .expect("db")
        .execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB);")
        .expect("table");
}

fn tokens(access: &str, refresh: &str, email: Option<&str>) -> TokenSet {
    let _ = email;
    TokenSet {
        access_token: access.to_string(),
        refresh_token: Some(refresh.to_string()),
        id_token: None,
        expires_at: Some(1_800_000_000),
        token_type: "Bearer".to_string(),
    }
}

fn request<'a>(tokens: &'a TokenSet, email: Option<&'a str>) -> SwitchRequest<'a> {
    SwitchRequest {
        agent: "antigravity",
        tokens,
        provider_state: None,
        display_name: email.unwrap_or("Antigravity"),
        account_id: None,
        email,
    }
}

#[test]
fn writes_the_projection_and_reads_it_back() {
    let fixture = Fixture::sandboxed().with_database();
    let tokens = tokens("at-1", "rt-1", Some("dev@example.com"));
    let outcome = write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");

    assert_eq!(outcome.agent, "antigravity");
    assert_eq!(outcome.live.as_deref(), Some("dev@example.com"));
    assert_eq!(outcome.target, fixture.db.display().to_string());
    assert!(
        fixture
            .db
            .starts_with(fixture.dir.path().join("tool")),
        "the sandboxed user-data path is the one written: {}",
        fixture.db.display()
    );

    let local = read(&fixture.roots).expect("read").expect("a session");
    assert_eq!(local.tokens.access_token, "at-1");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-1"));
    assert_eq!(local.tokens.expires_at, Some(1_800_000_000));
    assert_eq!(local.identity.email.as_deref(), Some("dev@example.com"));
    assert_eq!(local.source, fixture.db.display().to_string());
}

#[test]
fn the_row_is_a_base64_protobuf_carrying_the_sentinel() {
    let fixture = Fixture::sandboxed().with_database();
    let tokens = tokens("at-1", "rt-1", None);
    write(&fixture.roots, &request(&tokens, Some("dev@example.com"))).expect("write");

    let encoded = fixture.row().expect("the oauthToken row");
    let blob = STANDARD.decode(encoded.trim()).expect("base64");
    let decoded = String::from_utf8_lossy(&blob);
    assert!(
        decoded.contains("oauthTokenInfoSentinelKey"),
        "the row is not the unified-state topic"
    );
    let session = super::read_session(&fixture.db).expect("session").expect("readable");
    assert_eq!(session.access_token, "at-1");
    assert_eq!(session.refresh_token, "rt-1");
    assert_eq!(session.email.as_deref(), Some("dev@example.com"));
}

#[test]
fn a_sandboxed_switch_still_needs_the_state_database() {
    // 沙箱里不写系统凭据，投影就是唯一落点：库不在场时无从写起。
    let fixture = Fixture::sandboxed();
    let tokens = tokens("at-1", "rt-1", None);
    let error = write(&fixture.roots, &request(&tokens, None)).expect_err("no database");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");
    assert!(error.to_string().contains("state.vscdb"), "{error}");
    assert!(!fixture.db.exists(), "a stand-in database is never created");
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn a_session_without_both_tokens_is_refused() {
    let fixture = Fixture::sandboxed().with_database();
    let mut missing_refresh = tokens("at-1", "rt-1", None);
    missing_refresh.refresh_token = Some("  ".to_string());
    let error = write(&fixture.roots, &request(&missing_refresh, None)).expect_err("no refresh");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");

    let mut missing_access = tokens("at-1", "rt-1", None);
    missing_access.access_token = " ".to_string();
    let error = write(&fixture.roots, &request(&missing_access, None)).expect_err("no access");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");
    assert_eq!(fixture.row(), None, "nothing was written");
}

#[test]
fn snapshot_puts_the_previous_session_back() {
    let fixture = Fixture::sandboxed().with_database();
    let first = tokens("at-a", "rt-a", None);
    write(&fixture.roots, &request(&first, Some("a@example.com"))).expect("first write");

    let captured = switch::snapshot_with(&fixture.roots, "antigravity").expect("snapshot");
    let second = tokens("at-b", "rt-b", None);
    write(&fixture.roots, &request(&second, Some("b@example.com"))).expect("second write");
    assert_eq!(
        read(&fixture.roots).expect("read").expect("session").tokens.access_token,
        "at-b"
    );

    switch::restore(&captured).expect("restore");
    assert_eq!(
        read(&fixture.roots).expect("read").expect("session").tokens.access_token,
        "at-a"
    );
}

#[test]
fn restoring_a_session_that_never_existed_drops_the_row() {
    let fixture = Fixture::sandboxed().with_database();
    let captured = switch::snapshot_with(&fixture.roots, "antigravity").expect("snapshot");
    let tokens = tokens("at-temp", "rt-temp", None);
    write(&fixture.roots, &request(&tokens, None)).expect("write");
    assert!(fixture.row().is_some());

    switch::restore(&captured).expect("restore");
    assert_eq!(fixture.row(), None);
    assert!(read(&fixture.roots).expect("read").is_none());
}

#[test]
fn a_database_that_rejects_the_row_fails_the_switch_and_keeps_the_old_session() {
    let fixture = Fixture::sandboxed().with_database();
    let previous = tokens("at-old", "rt-old", None);
    write(&fixture.roots, &request(&previous, Some("old@example.com"))).expect("write");

    fixture
        .connection()
        .execute_batch(
            "CREATE TRIGGER reject_session BEFORE INSERT ON ItemTable
             WHEN NEW.key = 'antigravityUnifiedStateSync.oauthToken'
              AND NEW.value <> (SELECT value FROM ItemTable WHERE key = NEW.key)
             BEGIN SELECT RAISE(ABORT, 'fixture rejects a different session'); END;",
        )
        .expect("trigger");

    let next = tokens("at-new", "rt-new", None);
    let error = write(&fixture.roots, &request(&next, None)).expect_err("the trigger refuses");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");

    let session = super::read_session(&fixture.db)
        .expect("session")
        .expect("the old session is still readable");
    assert_eq!(session.access_token, "at-old");
    assert_eq!(session.refresh_token, "rt-old");
}

#[test]
fn live_reports_the_email_and_the_token_fingerprints() {
    let fixture = Fixture::sandboxed().with_database();
    let tokens = tokens("at-live", "rt-live", None);
    write(&fixture.roots, &request(&tokens, Some("live@example.com"))).expect("write");

    let live = switch::live_with(&fixture.roots, "antigravity").expect("live");
    assert_eq!(live.identity.as_deref(), Some("live@example.com"));
    assert_eq!(
        live.access_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("at-live").as_str())
    );
    assert_eq!(
        live.refresh_fingerprint.as_deref(),
        Some(crate::identity::fingerprint("rt-live").as_str())
    );
    assert_eq!(live.source, fixture.db.display().to_string());
}

#[test]
fn the_env_sandbox_keeps_a_real_switch_out_of_the_real_home() {
    let sandbox = crate::test_support::Sandbox::new();
    let path = credential_path(&PathRoots::from_env());
    assert!(
        path.starts_with(sandbox.tool_path(&[])),
        "{} escaped the sandbox",
        path.display()
    );
    create_database(&path);

    let tokens = tokens("at-env", "rt-env", None);
    switch::write(&request(&tokens, Some("env@example.com"))).expect("write through the environment");
    let live = switch::live("antigravity").expect("live through the environment");
    assert_eq!(live.identity.as_deref(), Some("env@example.com"));
    assert_eq!(live.source, path.display().to_string());
}

#[test]
fn reconcile_matches_the_serving_session_by_email_and_by_token() {
    use crate::oauth::Identity;
    use crate::record::Account;
    use crate::store::{self, IndexEntry};

    // 对账要读账号库，所以这里用环境沙箱而不是自造 roots。
    let _sandbox = crate::test_support::Sandbox::new();
    let roots = PathRoots::from_env();
    let db = credential_path(&roots);
    create_database(&db);
    let session = tokens("at-live", "rt-live", None);
    write(&roots, &request(&session, Some("Live@Example.com"))).expect("write");
    let live = switch::live_with(&roots, "antigravity").expect("live");
    assert_eq!(live.identity.as_deref(), Some("Live@Example.com"));

    // 邮箱不区分大小写就是同一个账号。
    let by_email = Account::from_login(
        "antigravity",
        &Identity {
            email: Some("live@example.com".to_string()),
            ..Identity::default()
        },
        &session,
        None,
        1_000,
    )
    .expect("record");
    store::upsert(&by_email).expect("store");
    let entries = vec![IndexEntry::from_account(&by_email)];
    assert_eq!(
        crate::reconcile::match_live(&entries, "antigravity", &live).as_deref(),
        Some(by_email.id.as_str())
    );

    // 客户端没露出邮箱时，手里的 token 指纹就是最后的线索。
    let by_token =
        Account::from_login("antigravity", &Identity::default(), &session, None, 1_000)
            .expect("record");
    store::upsert(&by_token).expect("store");
    let entries = vec![IndexEntry::from_account(&by_token)];
    assert_eq!(
        crate::reconcile::match_live(&entries, "antigravity", &live).as_deref(),
        Some(by_token.id.as_str())
    );

    // 既不认同一个邮箱也认不出 token 的账号不会被误认。
    let unrelated = Account::from_login(
        "antigravity",
        &Identity {
            email: Some("someone@example.com".to_string()),
            ..Identity::default()
        },
        &tokens("at-other", "rt-other", None),
        None,
        1_000,
    )
    .expect("record");
    store::upsert(&unrelated).expect("store");
    let entries = vec![IndexEntry::from_account(&unrelated)];
    assert_eq!(
        crate::reconcile::match_live(&entries, "antigravity", &live),
        None
    );
}

// ---------------------------------------------------------------------------
// 系统凭据：Keychain + ~/.gemini 文件（全部走注入的执行器）
// ---------------------------------------------------------------------------

/// 一个记录并模拟 `security` 的执行器。
#[cfg(unix)]
struct FakeRunner {
    /// 让这个子命令失败（其余照常）。
    fail: Option<&'static str>,
    stored: std::cell::RefCell<Option<String>>,
    log: std::cell::RefCell<Vec<Vec<String>>>,
}

#[cfg(unix)]
impl FakeRunner {
    fn new() -> Self {
        Self {
            fail: None,
            stored: std::cell::RefCell::new(None),
            log: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn failing(command: &'static str) -> Self {
        Self {
            fail: Some(command),
            ..Self::new()
        }
    }

    /// 钥匙串里已经有一条客户端自己写的条目。
    fn keeping(value: &str) -> Self {
        let runner = Self::new();
        *runner.stored.borrow_mut() = Some(value.to_string());
        runner
    }

    fn calls(&self) -> Vec<Vec<String>> {
        self.log.borrow().clone()
    }

    fn stored(&self) -> Option<String> {
        self.stored.borrow().clone()
    }
}

#[cfg(unix)]
impl super::antigravity_credential::CommandRunner for FakeRunner {
    fn run(&self, program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::unix::process::ExitStatusExt;
        let mut call = vec![program.to_string()];
        call.extend(args.iter().map(|arg| (*arg).to_string()));
        self.log.borrow_mut().push(call.clone());
        let exit = |code: i32, stdout: &str, stderr: &str| std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        };
        match args.first().copied() {
            Some("find-generic-password") => match self.stored() {
                Some(value) => Ok(exit(0, &value, "")),
                None => Ok(exit(44, "", "could not be found")),
            },
            Some("add-generic-password") => {
                if self.fail == Some("add-generic-password") {
                    return Ok(exit(1, "", "user denied"));
                }
                if let Some(value) = args.last() {
                    *self.stored.borrow_mut() = Some((*value).to_string());
                }
                Ok(exit(0, "", ""))
            }
            Some("delete-generic-password") => {
                *self.stored.borrow_mut() = None;
                Ok(exit(0, "", ""))
            }
            _ => Ok(exit(1, "", "unsupported")),
        }
    }
}

#[cfg(unix)]
#[test]
fn the_macos_path_writes_the_keychain_and_both_gemini_files() {
    let fixture = Fixture::macos().with_database();
    let cli_session = fixture.with_cli_session_dir();
    let runner = FakeRunner::new();

    let tokens = tokens("at-1", "rt-1", None);
    let outcome = super::write_with_runner(
        &fixture.roots,
        &request(&tokens, Some("dev@example.com")),
        &runner,
    )
    .expect("write");

    assert_eq!(outcome.agent, "antigravity");
    // Keychain：go-keyring-base64 包装，不带 -A/-T。
    let stored = runner.stored().expect("the keychain value");
    let add = runner
        .calls()
        .into_iter()
        .find(|call| call[1] == "add-generic-password")
        .expect("the add call");
    assert!(
        add.iter().any(|arg| arg == "gemini") && add.iter().any(|arg| arg == "antigravity"),
        "{add:?}"
    );
    assert!(!add.iter().any(|arg| arg == "-A" || arg == "-T"), "{add:?}");
    let payload = super::antigravity_credential::parse_keychain_value(&stored).expect("payload");
    assert!(payload.contains("\"refresh_token\":\"rt-1\""), "{payload}");
    assert!(payload.contains("\"auth_method\":\"consumer\""), "{payload}");

    // 回退文件与 CLI 会话文件是同一份载荷。
    let fallback = std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
        .expect("fallback file");
    assert_eq!(fallback, payload);
    assert_eq!(std::fs::read_to_string(cli_session).expect("cli session"), payload);

    // 库里的投影也写好了。
    assert_eq!(
        super::read_session(&fixture.db).expect("session").expect("row").refresh_token,
        "rt-1"
    );
}

#[cfg(unix)]
#[test]
fn the_keychain_is_read_before_the_database() {
    let fixture = Fixture::macos().with_database();
    let runner = FakeRunner::new();
    let tokens = tokens("at-keychain", "rt-keychain", None);
    super::write_with_runner(&fixture.roots, &request(&tokens, None), &runner).expect("write");

    // 数据库里换成别的会话：Keychain 才是权威来源。
    write_row(&fixture, "at-database", "rt-database", None);

    let local = read_with_runner(&fixture.roots, &runner)
        .expect("read")
        .expect("a session");
    assert_eq!(local.tokens.access_token, "at-keychain");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-keychain"));
    assert!(local.source.contains("Keychain"), "{}", local.source);
}

/// 收下写入却读不回来的执行器：模拟钥匙串静默丢写。
#[cfg(unix)]
struct ForgetfulRunner;

#[cfg(unix)]
impl super::antigravity_credential::CommandRunner for ForgetfulRunner {
    fn run(&self, _program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::unix::process::ExitStatusExt;
        let code = match args.first().copied() {
            Some("find-generic-password") => 44,
            _ => 0,
        };
        Ok(std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: b"could not be found".to_vec(),
        })
    }
}

#[cfg(unix)]
#[test]
fn a_client_without_a_state_database_switches_through_the_keychain() {
    // 2.x 桌面端不再创建 state.vscdb：只有系统凭据的那台机器也必须能切。
    let fixture = Fixture::macos();
    let runner = FakeRunner::new();
    let tokens = tokens("at-2x", "rt-2x", None);

    let outcome =
        super::write_with_runner(&fixture.roots, &request(&tokens, Some("dev@example.com")), &runner)
            .expect("write");

    assert!(
        !fixture.db.exists(),
        "a missing state.vscdb must not be created: {}",
        fixture.db.display()
    );
    assert!(outcome.target.contains("Keychain"), "{}", outcome.target);
    assert_eq!(outcome.live.as_deref(), Some("dev@example.com"));

    // 客户端读的那一处（系统凭据）就是这次会话。
    let local = read_with_runner(&fixture.roots, &runner)
        .expect("read")
        .expect("a session");
    assert_eq!(local.tokens.access_token, "at-2x");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-2x"));
    assert!(local.source.contains("Keychain"), "{}", local.source);
    // 回退文件照旧写：Keyring 超时时客户端会退到它。
    assert!(
        fixture.gemini().join("jetski-standalone-oauth-token").is_file(),
        "the fallback file is part of the session"
    );
}

#[cfg(unix)]
#[test]
fn a_switch_keeps_the_fields_the_client_put_in_the_keychain() {
    // 客户端自己写的载荷比我们这份多 writer_id / token.expires_in 这类字段：
    // 切号只覆盖 token 与 id_token_email，别的原样留着。
    let existing = r#"{"token":{"access_token":"old-at","token_type":"Bearer","refresh_token":"old-rt","expiry":"2020-01-01T00:00:00.000000Z","expires_in":3599},"auth_method":"consumer","writer_id":"w-1","id_token_email":"old@example.com"}"#;
    let runner = FakeRunner::keeping(&super::antigravity_credential::keychain_value(existing));
    let fixture = Fixture::macos();
    let tokens = tokens("at-new", "rt-new", None);

    super::write_with_runner(
        &fixture.roots,
        &request(&tokens, Some("new@example.com")),
        &runner,
    )
    .expect("write");

    let stored = runner.stored().expect("the keychain value");
    let payload =
        super::antigravity_credential::parse_keychain_value(&stored).expect("payload");
    let value: serde_json::Value = serde_json::from_str(&payload).expect("json");
    assert_eq!(value["token"]["refresh_token"], "rt-new", "{payload}");
    assert_eq!(value["writer_id"], "w-1", "{payload}");
    assert_eq!(value["token"]["expires_in"], 3599, "{payload}");
    assert_eq!(value["id_token_email"], "new@example.com", "{payload}");
    // 回退文件拿的是同一份合并后的载荷。
    assert_eq!(
        std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
            .expect("fallback file"),
        payload
    );
}

#[cfg(unix)]
#[test]
fn a_keychain_that_forgets_the_session_fails_the_switch() {
    // 没有 vscdb 可回读时，系统凭据的回读就是唯一的提交校验。
    let fixture = Fixture::macos();
    let tokens = tokens("at-1", "rt-1", None);
    let error =
        super::write_with_runner(&fixture.roots, &request(&tokens, None), &ForgetfulRunner)
            .expect_err("the keychain lost the write");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");
    assert!(error.to_string().contains("Keychain"), "{error}");
    assert!(
        !fixture.gemini().join("jetski-standalone-oauth-token").exists(),
        "a refused switch leaves no half-written fallback file"
    );
    assert!(!fixture.db.exists(), "no database is invented on the way back");
}

#[cfg(unix)]
#[test]
fn a_keychain_failure_rolls_every_store_back() {
    let fixture = Fixture::macos().with_database();
    let previous = tokens("at-old", "rt-old", None);
    write_with_runner(&fixture.roots, &request(&previous, Some("old@example.com")), &FakeRunner::new())
        .expect("write the previous session");
    let old_row = fixture.row();
    let old_payload = std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
        .expect("fallback file");

    let next = tokens("at-new", "rt-new", None);
    let runner = FakeRunner::failing("add-generic-password");
    let error = write_with_runner(&fixture.roots, &request(&next, None), &runner)
        .expect_err("the keychain refuses");
    assert!(matches!(error, crate::AccountError::Client(_)), "{error:?}");

    assert_eq!(fixture.row(), old_row, "the database row was not put back");
    assert_eq!(
        std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
            .expect("fallback file"),
        old_payload,
        "the fallback file was not put back"
    );
    assert_eq!(
        super::read_session(&fixture.db).expect("session").expect("row").refresh_token,
        "rt-old"
    );
}

#[cfg(unix)]
#[test]
fn snapshots_cover_the_keychain_and_the_files() {
    let fixture = Fixture::macos().with_database();
    // 一个执行器贯穿全程，模拟同一台机器上同一个钥匙串。
    let runner = FakeRunner::new();
    let first = tokens("at-a", "rt-a", None);
    write_with_runner(&fixture.roots, &request(&first, Some("a@example.com")), &runner)
        .expect("write a");
    let captured = snapshot_with_runner(&fixture.roots, &runner).expect("snapshot");
    let ClientSnapshot::Antigravity(snapshot) = captured.clone() else {
        panic!("antigravity snapshots carry all four stores");
    };

    let second = tokens("at-b", "rt-b", None);
    write_with_runner(&fixture.roots, &request(&second, Some("b@example.com")), &runner)
        .expect("write b");
    assert_eq!(
        super::read_session(&fixture.db).expect("session").expect("row").refresh_token,
        "rt-b"
    );

    super::restore_with_runner(&snapshot, &runner).expect("restore");
    assert_eq!(
        super::read_session(&fixture.db).expect("session").expect("row").refresh_token,
        "rt-a"
    );
    let restored = super::antigravity_credential::parse_keychain_value(
        &runner.stored().expect("keychain value"),
    )
    .expect("payload");
    assert!(restored.contains("\"refresh_token\":\"rt-a\""), "{restored}");
}

#[cfg(unix)]
#[test]
fn a_keychain_entry_that_never_existed_is_deleted_on_rollback() {
    let fixture = Fixture::macos().with_database();
    let runner = FakeRunner::new();
    let captured = snapshot_with_runner(&fixture.roots, &runner).expect("snapshot");
    let ClientSnapshot::Antigravity(snapshot) = captured else {
        panic!("antigravity snapshots carry all four stores");
    };
    assert!(runner.stored().is_none());

    let tokens = tokens("at-1", "rt-1", None);
    let writer = FakeRunner::new();
    write_with_runner(&fixture.roots, &request(&tokens, None), &writer).expect("write");
    assert!(writer.stored().is_some());

    super::restore_with_runner(&snapshot, &writer).expect("restore");
    assert!(writer.stored().is_none(), "the keychain entry was left behind");
    assert!(
        super::read_session(&fixture.db).expect("session").is_none(),
        "a session that never existed must be dropped"
    );
    assert!(
        !fixture.gemini().join("jetski-standalone-oauth-token").exists(),
        "the fallback file must be removed"
    );
}

#[cfg(unix)]
#[test]
fn the_snapshot_survives_a_missing_gemini_home() {
    let fixture = Fixture::macos().with_database();
    assert!(!fixture.gemini().exists());
    let runner = FakeRunner::new();
    let tokens = tokens("at-1", "rt-1", None);
    write_with_runner(&fixture.roots, &request(&tokens, None), &runner).expect("write");
    assert!(fixture.home().join(".gemini").is_dir(), "the gemini home is created");
    let local = read_with_runner(&fixture.roots, &FakeRunner::new())
        .expect("read")
        .expect("a session");
    assert_eq!(local.tokens.access_token, "at-1");
}

#[cfg(unix)]
#[test]
fn the_antigravity_read_falls_back_to_the_database_when_the_keychain_is_empty() {
    let fixture = Fixture::macos().with_database();
    write_row(&fixture, "at-database", "rt-database", Some("db@example.com"));
    let local = read_with_runner(&fixture.roots, &FakeRunner::new())
        .expect("read")
        .expect("a session");
    assert_eq!(local.tokens.access_token, "at-database");
    assert_eq!(local.identity.email.as_deref(), Some("db@example.com"));
}

#[derive(Default)]
struct FakeWinCred {
    stored: std::cell::RefCell<Option<(String, String, Vec<u8>)>>,
    fail_write: bool,
    forget_on_read: bool,
}

impl FakeWinCred {
    fn new() -> Self {
        Self::default()
    }

    fn keeping(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            stored: std::cell::RefCell::new(Some((
                super::antigravity_wincred::WINCRED_TARGET.to_string(),
                super::antigravity_wincred::WINCRED_USERNAME.to_string(),
                bytes.into(),
            ))),
            fail_write: false,
            forget_on_read: false,
        }
    }

    fn stored_bytes(&self) -> Option<Vec<u8>> {
        self.stored
            .borrow()
            .as_ref()
            .map(|(_, _, blob)| blob.clone())
    }

    fn stored_record(&self) -> Option<(String, String, Vec<u8>)> {
        self.stored.borrow().clone()
    }
}

impl super::antigravity_wincred::WinCredStore for FakeWinCred {
    fn read(&self, target: &str) -> crate::AccountResult<Option<Vec<u8>>> {
        if self.forget_on_read {
            return Ok(None);
        }
        Ok(self
            .stored
            .borrow()
            .as_ref()
            .filter(|(stored_target, _, _)| stored_target == target)
            .map(|(_, _, blob)| blob.clone()))
    }

    fn write(&self, target: &str, username: &str, blob: &[u8]) -> crate::AccountResult<()> {
        if self.fail_write {
            return Err(crate::AccountError::Client("WinCred write refused".to_string()));
        }
        *self.stored.borrow_mut() = Some((target.to_string(), username.to_string(), blob.to_vec()));
        Ok(())
    }

    fn delete(&self, target: &str) -> crate::AccountResult<()> {
        let should_clear = self
            .stored
            .borrow()
            .as_ref()
            .is_some_and(|(stored_target, _, _)| stored_target == target);
        if should_clear {
            *self.stored.borrow_mut() = None;
        }
        Ok(())
    }
}

struct NoopRunner;

impl super::antigravity_credential::CommandRunner for NoopRunner {
    fn run(&self, _program: &str, _args: &[&str]) -> std::io::Result<std::process::Output> {
        Err(std::io::Error::other("unexpected macOS security call on Windows"))
    }
}

fn win_stores<'a>(wincred: &'a FakeWinCred, runner: &'a NoopRunner) -> super::SystemStores<'a> {
    super::SystemStores { runner, wincred }
}

#[test]
fn windows_2x_write_and_read_wincred_without_vscdb() {
    let fixture = Fixture::windows();
    let cli_session = fixture.with_cli_session_dir();
    let wincred = FakeWinCred::new();
    let runner = NoopRunner;
    let stores = win_stores(&wincred, &runner);

    let tokens = tokens("at-win-1", "rt-win-1", None);
    let outcome = super::write_with_stores(&fixture.roots, &request(&tokens, Some("win@example.com")), stores)
        .expect("windows write");
    assert!(!fixture.db.exists(), "2.x does not invent state.vscdb");
    assert!(outcome.target.contains("Windows WinCred"), "{}", outcome.target);

    let (target, user, blob) = wincred.stored_record().expect("wincred entry");
    assert_eq!(target, "gemini:antigravity");
    assert_eq!(user, "antigravity");
    let json = std::str::from_utf8(&blob).expect("utf8 json");
    assert!(!json.starts_with("go-keyring-base64:"), "WinCred writes raw UTF-8 JSON: {json}");
    assert!(json.contains("\"refresh_token\":\"rt-win-1\""), "{json}");
    assert!(fixture.gemini().join("jetski-standalone-oauth-token").is_file());
    assert!(cli_session.is_file());

    let local = super::read_with_stores(&fixture.roots, stores)
        .expect("read")
        .expect("session");
    assert_eq!(local.tokens.access_token, "at-win-1");
    assert_eq!(local.tokens.refresh_token.as_deref(), Some("rt-win-1"));
    assert!(local.source.contains("Windows WinCred"), "{}", local.source);
}

#[test]
fn windows_wincred_merges_existing_fields_and_reads_go_keyring_base64() {
    let existing = r#"{"token":{"access_token":"old-at","token_type":"Bearer","refresh_token":"old-rt","expiry":"2020-01-01T00:00:00.000000Z","expires_in":3599},"auth_method":"consumer","writer_id":"win-writer","id_token_email":"old@example.com"}"#;
    let wrapped = super::antigravity_credential::keychain_value(existing);
    let wincred = FakeWinCred::keeping(wrapped.into_bytes());
    let runner = NoopRunner;
    let stores = win_stores(&wincred, &runner);
    let fixture = Fixture::windows();

    let before = super::read_with_stores(&fixture.roots, stores)
        .expect("read base64 wincred")
        .expect("session");
    assert_eq!(before.tokens.refresh_token.as_deref(), Some("old-rt"));
    assert_eq!(before.identity.email.as_deref(), Some("old@example.com"));

    let next = tokens("at-win-new", "rt-win-new", None);
    super::write_with_stores(&fixture.roots, &request(&next, Some("new@example.com")), stores)
        .expect("merge write");
    let blob = wincred.stored_bytes().expect("blob");
    let value: serde_json::Value = serde_json::from_slice(&blob).expect("json");
    assert_eq!(value["token"]["refresh_token"], "rt-win-new");
    assert_eq!(value["token"]["expires_in"], 3599);
    assert_eq!(value["writer_id"], "win-writer");
    assert_eq!(value["id_token_email"], "new@example.com");
}

#[test]
fn windows_read_falls_back_to_gemini_file_and_then_vscdb() {
    let fixture = Fixture::windows().with_database();
    let empty_wincred = FakeWinCred::new();
    let runner = NoopRunner;
    let stores = win_stores(&empty_wincred, &runner);

    write_row(&fixture, "at-db", "rt-db", Some("db@example.com"));
    let fallback_path = fixture.gemini().join("jetski-standalone-oauth-token");
    std::fs::create_dir_all(fallback_path.parent().expect("gemini dir")).expect("dir");
    let fallback_payload =
        super::antigravity_credential::payload_json("at-fallback", "rt-fallback", 1_800_000_000, None);
    std::fs::write(&fallback_path, fallback_payload).expect("seed fallback");

    // Empty WinCred falls back to ~/.gemini/jetski-standalone-oauth-token before state.vscdb.
    let from_fallback = super::read_with_stores(&fixture.roots, stores)
        .expect("read fallback")
        .expect("session");
    assert_eq!(from_fallback.tokens.access_token, "at-fallback");
    assert_eq!(from_fallback.tokens.refresh_token.as_deref(), Some("rt-fallback"));
    assert!(from_fallback.source.contains("jetski-standalone-oauth-token"), "{}", from_fallback.source);

    // Without the fallback file, empty WinCred falls through to state.vscdb.
    std::fs::remove_file(&fallback_path).expect("remove fallback");
    let from_db = super::read_with_stores(&fixture.roots, stores)
        .expect("read db")
        .expect("db session");
    assert_eq!(from_db.tokens.access_token, "at-db");
    assert_eq!(from_db.identity.email.as_deref(), Some("db@example.com"));
}

#[test]
fn windows_wincred_failure_rolls_back_all_stores_and_cleans_new_entries() {
    let fixture = Fixture::windows().with_database();
    let runner = NoopRunner;
    let wincred = FakeWinCred::new();
    let previous = tokens("at-old", "rt-old", None);
    super::write_with_stores(
        &fixture.roots,
        &request(&previous, Some("old@example.com")),
        win_stores(&wincred, &runner),
    )
    .expect("seed old");
    let old_blob = wincred.stored_bytes().expect("old blob");
    let old_fallback = std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
        .expect("old fallback");
    let old_row = fixture.row();

    // Case 1: write fails -> everything restored.
    let failing = FakeWinCred {
        stored: std::cell::RefCell::new(wincred.stored.borrow().clone()),
        fail_write: true,
        forget_on_read: false,
    };
    let next = tokens("at-new", "rt-new", None);
    let err = super::write_with_stores(
        &fixture.roots,
        &request(&next, Some("new@example.com")),
        win_stores(&failing, &runner),
    )
    .expect_err("write failure");
    assert!(matches!(err, crate::AccountError::Client(_)), "{err:?}");
    assert_eq!(failing.stored_bytes().expect("restored blob"), old_blob);
    assert_eq!(
        std::fs::read_to_string(fixture.gemini().join("jetski-standalone-oauth-token"))
            .expect("restored fallback"),
        old_fallback
    );
    assert_eq!(fixture.row(), old_row);

    // Case 2: absent prior to write -> snapshot restore deletes created entry & fallback file.
    let absent_fixture = Fixture::windows();
    let forgetful = FakeWinCred {
        stored: std::cell::RefCell::new(None),
        fail_write: false,
        forget_on_read: true,
    };
    let err2 = super::write_with_stores(
        &absent_fixture.roots,
        &request(&next, None),
        win_stores(&forgetful, &runner),
    )
    .expect_err("verify failure triggers rollback");
    assert!(matches!(err2, crate::AccountError::Client(_)), "{err2:?}");
    assert!(forgetful.stored_bytes().is_none(), "WinCred should be deleted on rollback");
    assert!(
        !absent_fixture.gemini().join("jetski-standalone-oauth-token").exists(),
        "fallback file should be deleted on rollback"
    );
}
