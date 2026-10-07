//! Behavior tests for the opencode parser (slice 06's three database
//! fixtures — v1, v2 with the migration completed, migration under way —
//! plus the older JSON storage fallback).
//!
//! Every database fixture is built inside a temp directory with rusqlite at
//! test time (schemas from OpenCode's own packages/core/src/session/sql.ts,
//! as magpie's tests build them); no developer database is ever copied.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::opencode::OpenCodeParser;
use super::{SessionCall, SessionFile, SessionParser, SessionTokens};
use crate::test_support::EnvGuard;

const MAIN: &str = "ses_1a2b3c4d5e6fAAAAAAAAAAAAAA";
const CHILD: &str = "ses_1a2b3c4d5e6fBBBBBBBBBBBBBB";
const IDLE: &str = "ses_1a2b3c4d5e6fCCCCCCCCCCCCCC";

const MAIN_USER: u64 = 1790416805000;
const MAIN_CREATED_1: u64 = 1790416806000;
const MAIN_COMPLETED_1: u64 = 1790416830000;
const MAIN_CREATED_2: u64 = 1790416890000;

/// Sandbox home + data, returns (home_dir, data_dir, guard).
fn sandbox() -> (tempfile::TempDir, tempfile::TempDir, EnvGuard) {
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let guard = EnvGuard::set(&[
        ("SKILLSTAR_TOOL_SYNC_HOME", home.path()),
        ("SKILLSTAR_DATA_DIR", data.path()),
    ]);
    (home, data, guard)
}

/// The sandbox's opencode data directory (`<home>/.local/share/opencode`,
/// the XDG default layout).
fn oc_data(home: &Path) -> PathBuf {
    home.join(".local").join("share").join("opencode")
}

/// Create the old tables (session/message/part) in a new database at `path`.
fn v1_db(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    for sql in [
        "CREATE TABLE session (id text PRIMARY KEY, project_id text NOT NULL, parent_id text, slug text NOT NULL, directory text NOT NULL, title text NOT NULL, version text NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL, time_archived integer, tokens_input integer DEFAULT 0 NOT NULL)",
        "CREATE TABLE message (id text PRIMARY KEY, session_id text NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL, data text NOT NULL)",
        "CREATE TABLE part (id text PRIMARY KEY, message_id text NOT NULL, session_id text NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL, data text NOT NULL)",
    ] {
        conn.execute(sql, []).unwrap();
    }
    conn
}

fn insert_v1_session(conn: &Connection, id: &str, parent: Option<&str>, directory: &str) {
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, '0f3a', ?2, 's', ?3, 't', '1.1.53', 1790416800000, 1790416800000)",
        rusqlite::params![id, parent, directory],
    )
    .unwrap();
}

fn insert_v1_message(conn: &Connection, id: &str, sid: &str, created: u64, data: &str) {
    conn.execute(
        "INSERT INTO message VALUES (?1, ?2, ?3, ?3, ?4)",
        rusqlite::params![id, sid, created as i64, data],
    )
    .unwrap();
}

/// OpenCode 2's tables beside the old ones, with the kv row's phase.
fn v2_tables(conn: &Connection, migration_phase: &str) {
    for sql in [
        "CREATE TABLE session_v2 (id text PRIMARY KEY, project_id text NOT NULL, workspace_id text, parent_id text, slug text NOT NULL, directory text NOT NULL, path text, title text, version text NOT NULL, cost real DEFAULT 0 NOT NULL, tokens_input integer DEFAULT 0 NOT NULL, tokens_output integer DEFAULT 0 NOT NULL, tokens_reasoning integer DEFAULT 0 NOT NULL, tokens_cache_read integer DEFAULT 0 NOT NULL, tokens_cache_write integer DEFAULT 0 NOT NULL, agent text, model text, time_created integer NOT NULL, time_updated integer NOT NULL, time_archived integer)",
        "CREATE TABLE session_message (id text PRIMARY KEY, session_id text NOT NULL, type text NOT NULL, seq integer NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL, data text NOT NULL)",
        "CREATE TABLE kv (key text PRIMARY KEY, value text NOT NULL, time_created integer NOT NULL, time_updated integer NOT NULL)",
    ] {
        conn.execute(sql, []).unwrap();
    }
    conn.execute(
        "INSERT INTO kv VALUES ('migration.v1-v2', ?1, 0, 0)",
        rusqlite::params![format!(r#"{{"phase":"{migration_phase}"}}"#)],
    )
    .unwrap();
}

fn insert_v2_session(conn: &Connection, id: &str, parent: Option<&str>) {
    conn.execute(
        "INSERT INTO session_v2 (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, 'global', ?2, 's', '/work/oc', NULL, '2.0.18', 1790416800000, 1790416800000)",
        rusqlite::params![id, parent],
    )
    .unwrap();
}

fn insert_v2_message(
    conn: &Connection,
    id: &str,
    sid: &str,
    kind: &str,
    seq: i64,
    created: u64,
    data: &str,
) {
    conn.execute(
        "INSERT INTO session_message VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
        rusqlite::params![id, sid, kind, seq, created as i64, data],
    )
    .unwrap();
}

fn session_file(agent_files: &[SessionFile], path: &Path) -> SessionFile {
    agent_files
        .iter()
        .find(|f| f.path == path)
        .cloned()
        .expect("file discovered")
}

fn tokens_of(calls: &[SessionCall]) -> Vec<SessionTokens> {
    calls.iter().map(|c| c.tokens).collect()
}

// ---------------------------------------------------------------------------
// The v1 database
// ---------------------------------------------------------------------------

#[test]
fn v1_database_sessions() {
    let (home, _data, _guard) = sandbox();
    let db = oc_data(home.path()).join("opencode.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = v1_db(&db);
    insert_v1_session(&conn, MAIN, None, "/work/oc");
    insert_v1_session(&conn, CHILD, Some(MAIN), "/work/oc");
    insert_v1_session(&conn, IDLE, None, "/work/oc");
    insert_v1_message(
        &conn,
        "msg_0",
        MAIN,
        MAIN_USER,
        r#"{"id":"msg_0","role":"user","time":{"created":1790416805000}}"#,
    );
    // modelId spelled in either case (real files spell it both ways).
    insert_v1_message(
        &conn,
        "msg_1",
        MAIN,
        MAIN_CREATED_1,
        r#"{"id":"msg_1","role":"assistant","modelId":"gpt-6-astra","tokens":{"input":1000,"output":100,"reasoning":20,"cache":{"read":5000,"write":0}},"time":{"created":1790416806000,"completed":1790416830000}}"#,
    );
    insert_v1_message(
        &conn,
        "msg_2",
        MAIN,
        MAIN_CREATED_2,
        r#"{"id":"msg_2","role":"assistant","modelID":"codex/gpt-6-luna","tokens":{"input":300,"output":50,"reasoning":10,"cache":{"read":2000,"write":0}},"time":{"created":1790416890000,"completed":1790416980000}}"#,
    );
    insert_v1_message(
        &conn,
        "msg_3",
        CHILD,
        1790416860000,
        r#"{"id":"msg_3","role":"assistant","modelID":"claude-opus-5-5","tokens":{"input":40,"output":60,"reasoning":0,"cache":{"read":700,"write":300}},"time":{"created":1790416860000,"completed":1790416890000}}"#,
    );
    drop(conn);

    let files = OpenCodeParser.discover(home.path());
    assert_eq!(files.len(), 3, "main, child and the idle session alike");
    let main_path = files
        .iter()
        .find(|f| {
            f.path
                .to_str()
                .is_some_and(|p| p.ends_with(&format!("#{MAIN}")))
        })
        .unwrap()
        .path
        .clone();
    let main_file = session_file(&files, &main_path);
    // "Size" is the message count.
    assert_eq!(main_file.size, 3);

    let (delta, checkpoint) = OpenCodeParser.parse(&main_file, None);
    assert_eq!(delta.len(), 2, "the user message is not a call");
    assert_eq!(
        tokens_of(&delta),
        vec![
            // opencode input already excludes the cache; reasoning rides
            // inside output (the codex.rs matrix).
            SessionTokens {
                input: 1000,
                output: 120,
                cache_read: 5000,
                cache_write: 0
            },
            SessionTokens {
                input: 300,
                output: 60,
                cache_read: 2000,
                cache_write: 0
            },
        ]
    );
    assert_eq!(delta[0].model_answered, "gpt-6-astra");
    assert_eq!(delta[0].session, MAIN);
    assert_eq!(
        delta[0].at as u64, MAIN_COMPLETED_1,
        "the completed time names the call"
    );
    assert_eq!(delta[0].latency_ms, Some(24000));
    assert_eq!(checkpoint.calls_seen, 2);
    // The view is fully rebuilt every run (rows are rewritten in place).
    let (again, _) = OpenCodeParser.parse(&main_file, Some(checkpoint.clone()));
    assert_eq!(tokens_of(&again), tokens_of(&delta));
    // replay returns the same full view, without dedup ids.
    let replayed = OpenCodeParser.replay(&checkpoint);
    assert_eq!(replayed.len(), 2);
    assert!(replayed.iter().all(|(id, _)| id.is_empty()));

    // The child session's calls are attributed to the root session.
    let child_path = main_path.with_file_name(format!("opencode.db#{CHILD}"));
    let child = session_file(&files, &child_path);
    let (child_delta, _) = OpenCodeParser.parse(&child, None);
    assert_eq!(child_delta.len(), 1);
    assert_eq!(
        child_delta[0].session, MAIN,
        "a subagent's work counts in the session it ran in"
    );
    assert_eq!(child_delta[0].tokens.input, 40);
}

// ---------------------------------------------------------------------------
// The v2 database with the migration completed
// ---------------------------------------------------------------------------

#[test]
fn v2_database_after_completed_migration() {
    let (home, _data, _guard) = sandbox();
    let db = oc_data(home.path()).join("opencode.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = v1_db(&db);
    // An old session OpenCode 2 deleted after copying it over: the kv row
    // says the migration completed, so the old tables are not read.
    insert_v1_session(&conn, "ses_gone", None, "/work/gone");
    insert_v1_message(
        &conn,
        "msg_gone",
        "ses_gone",
        1790416800000,
        r#"{"id":"msg_gone","role":"assistant","modelID":"claude-opus-5-5","tokens":{"input":1,"output":1,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1790416800000,"completed":1790416810000}}"#,
    );
    v2_tables(&conn, "completed");
    insert_v2_session(&conn, MAIN, None);
    insert_v2_session(&conn, CHILD, Some(MAIN));
    insert_v2_message(
        &conn,
        "msg_0001",
        MAIN,
        "user",
        0,
        MAIN_USER,
        r#"{"text":"Why does the login test flake?","time":{"created":1790416805000}}"#,
    );
    insert_v2_message(
        &conn,
        "msg_0002",
        MAIN,
        "assistant",
        1,
        MAIN_CREATED_1,
        r#"{"model":{"id":"gpt-6-astra","providerID":"openai"},"content":[{"type":"text","text":"Let me look."}],"cost":0.01,"tokens":{"input":1000,"output":100,"reasoning":20,"cache":{"read":5000,"write":0}},"time":{"created":1790416806000,"completed":1790416830000}}"#,
    );
    insert_v2_message(
        &conn,
        "msg_0003",
        MAIN,
        "assistant",
        2,
        MAIN_CREATED_2,
        r#"{"model":{"id":"codex/gpt-6-luna","providerID":"magpie"},"content":[],"tokens":{"input":300,"output":50,"reasoning":10,"cache":{"read":2000,"write":0}},"time":{"created":1790416890000,"completed":1790416980000}}"#,
    );
    // A compaction names no model: it counts on the one last replied with.
    insert_v2_message(
        &conn,
        "msg_0004",
        MAIN,
        "compaction",
        3,
        1790417000000,
        r#"{"status":"failed","reason":"auto","tokens":{"input":5,"output":1,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1790417000000}}"#,
    );
    insert_v2_message(
        &conn,
        "msg_0005",
        CHILD,
        "assistant",
        0,
        1790416860000,
        r#"{"model":{"id":"claude-opus-5-5","providerID":"anthropic"},"tokens":{"input":40,"output":60,"reasoning":0,"cache":{"read":700,"write":300}},"time":{"created":1790416860000,"completed":1790416890000}}"#,
    );
    drop(conn);

    let files = OpenCodeParser.discover(home.path());
    assert_eq!(
        files.len(),
        2,
        "the v2 sessions; the old 'gone' row is not read once the migration completed"
    );

    let main_path = files
        .iter()
        .find(|f| {
            f.path
                .to_str()
                .is_some_and(|p| p.ends_with(&format!("#{MAIN}")))
        })
        .unwrap()
        .path
        .clone();
    let (delta, checkpoint) = OpenCodeParser.parse(&session_file(&files, &main_path), None);
    assert_eq!(
        delta.len(),
        3,
        "two replies and the compaction; the user row is no call"
    );
    assert_eq!(
        delta[2].model_answered, "codex/gpt-6-luna",
        "the compaction takes the model last replied with"
    );
    assert_eq!(delta[2].tokens.input, 5);
    assert_eq!(checkpoint.calls_seen, 3);

    let child_path = main_path.with_file_name(format!("opencode.db#{CHILD}"));
    let (child_delta, _) = OpenCodeParser.parse(&session_file(&files, &child_path), None);
    assert_eq!(child_delta.len(), 1);
    assert_eq!(child_delta[0].session, MAIN);
}

// ---------------------------------------------------------------------------
// The migration under way
// ---------------------------------------------------------------------------

#[test]
fn migration_under_way_reads_uncopied_sessions_from_the_old_tables() {
    let (home, _data, _guard) = sandbox();
    let db = oc_data(home.path()).join("opencode.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = v1_db(&db);
    // Not yet copied over: read from the old tables.
    insert_v1_session(&conn, "ses_pending", None, "/work/pending");
    insert_v1_message(
        &conn,
        "msg_p1",
        "ses_pending",
        1790416800000,
        r#"{"id":"msg_p1","role":"assistant","modelID":"claude-opus-5-5","tokens":{"input":8,"output":4,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1790416800000,"completed":1790416809000}}"#,
    );
    v2_tables(&conn, "sessions"); // the phase is not "completed" yet
    // Already copied: its rows exist only in the v2 tables.
    insert_v2_session(&conn, MAIN, None);
    insert_v2_message(
        &conn,
        "msg_v2a",
        MAIN,
        "assistant",
        0,
        MAIN_CREATED_1,
        r#"{"model":{"id":"gpt-6-astra"},"tokens":{"input":1000,"output":100,"reasoning":20,"cache":{"read":5000,"write":0}},"time":{"created":1790416806000,"completed":1790416830000}}"#,
    );
    drop(conn);

    let files = OpenCodeParser.discover(home.path());
    assert_eq!(
        files.len(),
        2,
        "one session from each store, the copied one not double-listed"
    );
    let paths: Vec<String> = files
        .iter()
        .map(|f| {
            f.path
                .to_str()
                .unwrap()
                .rsplit('#')
                .next()
                .unwrap()
                .to_string()
        })
        .collect();
    assert!(paths.contains(&"ses_pending".to_string()));
    assert!(paths.contains(&MAIN.to_string()));

    let pending = files
        .iter()
        .find(|f| f.path.to_str().is_some_and(|p| p.ends_with("#ses_pending")))
        .unwrap();
    let (delta, _) = OpenCodeParser.parse(pending, None);
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].model_answered, "claude-opus-5-5");
    assert_eq!(delta[0].latency_ms, Some(9000));
}

// ---------------------------------------------------------------------------
// The JSON storage fallback
// ---------------------------------------------------------------------------

#[test]
fn json_storage_fallback_when_no_database() {
    let (home, _data, _guard) = sandbox();
    let storage = oc_data(home.path()).join("storage");
    let session_dir = storage.join("session").join("0f3a");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join(format!("{MAIN}.json")),
        format!(r#"{{"id":"{MAIN}","directory":"/work/oc","time":{{"created":1790416800000,"updated":1790416980000}}}}"#),
    )
    .unwrap();
    std::fs::write(
        session_dir.join(format!("{CHILD}.json")),
        format!(r#"{{"id":"{CHILD}","parentID":"{MAIN}","directory":"/work/oc","time":{{"created":1790416860000,"updated":1790416890000}}}}"#),
    )
    .unwrap();
    let main_messages = storage.join("message").join(MAIN);
    std::fs::create_dir_all(&main_messages).unwrap();
    std::fs::write(
        main_messages.join("msg_0001.json"),
        r#"{"id":"msg_0001","role":"user","time":{"created":1790416805000}}"#,
    )
    .unwrap();
    std::fs::write(main_messages.join("msg_0002.json"),
        r#"{"id":"msg_0002","role":"assistant","modelId":"gpt-6-astra","tokens":{"input":1000,"output":100,"reasoning":20,"cache":{"read":5000,"write":0}},"time":{"created":1790416806000,"completed":1790416830000}}"#).unwrap();
    let child_messages = storage.join("message").join(CHILD);
    std::fs::create_dir_all(&child_messages).unwrap();
    std::fs::write(child_messages.join("msg_0004.json"),
        r#"{"id":"msg_0004","role":"assistant","modelID":"claude-opus-5-5","tokens":{"input":40,"output":60,"reasoning":0,"cache":{"read":700,"write":300}},"time":{"created":1790416860000,"completed":1790416890000}}"#).unwrap();

    let files = OpenCodeParser.discover(home.path());
    assert_eq!(files.len(), 2, "the older JSON files are what there is");

    let main = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n == format!("{MAIN}.json").as_str())
        })
        .unwrap();
    let (delta, _) = OpenCodeParser.parse(main, None);
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].model_answered, "gpt-6-astra");
    assert_eq!(delta[0].tokens.output, 120);

    let child = files
        .iter()
        .find(|f| {
            f.path
                .file_name()
                .is_some_and(|n| n == format!("{CHILD}.json").as_str())
        })
        .unwrap();
    let (child_delta, _) = OpenCodeParser.parse(child, None);
    assert_eq!(child_delta.len(), 1);
    assert_eq!(
        child_delta[0].session, MAIN,
        "parentID links attribute to the root session"
    );
}

// ---------------------------------------------------------------------------
// Read-only opening and env handling
// ---------------------------------------------------------------------------

#[test]
fn parse_never_creates_or_writes_the_database() {
    let (home, _data, _guard) = sandbox();
    let db = oc_data(home.path()).join("opencode.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let missing = super::opencode_discovery::db_session_path(&db, "ses_none");
    let file = SessionFile {
        agent: OpenCodeParser::AGENT,
        path: missing.clone(),
        size: 0,
        modified_ms: 0,
    };
    let (delta, checkpoint) = OpenCodeParser.parse(&file, None);
    assert!(delta.is_empty());
    assert_eq!(checkpoint.calls_seen, 0);
    assert!(!db.exists(), "read-only opening never creates the database");
}

#[test]
fn opencode_db_env_named_and_sandbox_wins() {
    // Without the sandbox: $OPENCODE_DB names the database, relative to the
    // data folder when not absolute. A real v1 database there with one
    // session is found through it (had the env var been ignored, the default
    // opencode.db would be missing and nothing would be found).
    let data = tempfile::tempdir().unwrap();
    let oc = data.path().join("opencode");
    std::fs::create_dir_all(&oc).unwrap();
    let conn = v1_db(&oc.join("other.db"));
    insert_v1_session(&conn, MAIN, None, "/work/oc");
    insert_v1_message(
        &conn,
        "msg_1",
        MAIN,
        MAIN_CREATED_1,
        r#"{"id":"msg_1","role":"assistant","modelID":"gpt-6-astra","tokens":{"input":10,"output":2,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1790416806000,"completed":1790416830000}}"#,
    );
    drop(conn);

    let home = tempfile::tempdir().unwrap();
    // "other.db" as a path is the plain string; OPENCODE_DB takes it as a
    // relative name resolved against the data folder. The guard is scoped:
    // the next part needs the env lock back.
    let files = {
        let _guard = EnvGuard::set(&[
            ("XDG_DATA_HOME", data.path()),
            ("OPENCODE_DB", Path::new("other.db")),
        ]);
        OpenCodeParser.discover(home.path())
    };
    assert_eq!(files.len(), 1);
    assert!(
        files[0]
            .path
            .to_str()
            .is_some_and(|p| p.ends_with(&format!("other.db#{MAIN}")))
    );

    // The sandbox wins over both XDG_DATA_HOME and OPENCODE_DB.
    let sandbox_home = tempfile::tempdir().unwrap();
    let sandbox_data = tempfile::tempdir().unwrap();
    let sessions = sandbox_home
        .path()
        .join(".local")
        .join("share")
        .join("opencode")
        .join("storage")
        .join("session")
        .join("p");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::write(
        sessions.join("ses_sandbox.json"),
        r#"{"id":"ses_sandbox","time":{"created":1,"updated":1}}"#,
    )
    .unwrap();
    let _sandbox_guard = EnvGuard::set(&[
        ("SKILLSTAR_TOOL_SYNC_HOME", sandbox_home.path()),
        ("SKILLSTAR_DATA_DIR", sandbox_data.path()),
        ("XDG_DATA_HOME", data.path()),
        ("OPENCODE_DB", Path::new("other.db")),
    ]);
    let files = OpenCodeParser.discover(sandbox_home.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("ses_sandbox.json"));
}
