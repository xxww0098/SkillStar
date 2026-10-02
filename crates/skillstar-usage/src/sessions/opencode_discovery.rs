//! Session discovery for the opencode family.
//!
//! OpenCode has kept its sessions in an SQLite database since 1.2
//! (`opencode.db`: `session`, `message` and `part` tables, each message a
//! JSON document); OpenCode 2 writes its own tables (`session_v2`,
//! `session_message`) into the same database and copies every old session
//! over on first start, recording `kv["migration.v1-v2"] = {"phase":
//! "completed"}` when done and leaving the old tables behind unwritten.
//! Before the database existed, sessions were JSON files under `storage/`.
//!
//! Discovery therefore reads (magpie openCodeFiles precedent):
//!
//! 1. the database when `opencode.db` (or `$OPENCODE_DB`) exists: one
//!    pseudo-file per session, path `<db>#<session id>`, "size" the count of
//!    its messages and "modified" the latest change to any of them;
//! 2. otherwise the older JSON storage: one file per session under
//!    `storage/session/<project>/<id>.json`.
//!
//! v1/v2 disambiguation: rows come from `session_v2` when that table exists;
//! the old `session` rows are then read only while the migration is not
//! completed, excluding the ids already copied over.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use super::SessionFile;

/// OpenCode's data directory: `$XDG_DATA_HOME/opencode`, else
/// `<home>/.local/share/opencode` — on Windows too, where OpenCode keeps it
/// there (magpie OpenCodeDir precedent). The sandbox
/// (`SKILLSTAR_TOOL_SYNC_HOME`) always wins over `XDG_DATA_HOME`.
pub(super) fn opencode_data_dir(home: &Path) -> PathBuf {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return home.join(".local").join("share").join("opencode");
    }
    match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("opencode"),
        _ => home.join(".local").join("share").join("opencode"),
    }
}

/// OpenCode's database: `$OPENCODE_DB` (a path in the data folder unless
/// absolute; `:memory:` ignored), else `opencode.db` there. The sandbox wins
/// over `$OPENCODE_DB`.
fn opencode_db_path(home: &Path) -> PathBuf {
    if !crate::tool_paths::is_tool_sync_sandboxed()
        && let Some(var) = std::env::var_os("OPENCODE_DB")
        && !var.is_empty()
        && var != ":memory:"
    {
        let path = PathBuf::from(var);
        return if path.is_absolute() {
            path
        } else {
            opencode_data_dir(home).join(path)
        };
    }
    opencode_data_dir(home).join("opencode.db")
}

/// Open the database read-only (gateway agents/cindy.rs precedent); a
/// missing file, a hot write lock or any other failure counts as "no
/// database this run" (the caller falls back or returns nothing).
pub(super) fn open_read_only(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

/// Find opencode's sessions: the database's when it exists, else the older
/// JSON storage.
pub(super) fn opencode_files(agent: &'static str, home: &Path) -> Vec<SessionFile> {
    let db = opencode_db_path(home);
    if db.is_file() {
        return db_files(agent, &db);
    }
    json_files(agent, &opencode_data_dir(home).join("storage"))
}

/// Split a database pseudo-path `<db>#<session id>` back into its parts.
pub(super) fn split_db_path(path: &Path) -> Option<(PathBuf, String)> {
    let text = path.to_str()?;
    let (db, sid) = text.rsplit_once('#')?;
    Some((PathBuf::from(db), sid.to_string()))
}

/// Build a database pseudo-path.
pub(super) fn db_session_path(db: &Path, sid: &str) -> PathBuf {
    PathBuf::from(format!("{}#{sid}", db.display()))
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .is_ok_and(|n| n > 0)
}

/// Whether OpenCode 2 has copied every old session over (the kv row's phase
/// says "completed"; magpie ocMigrated precedent).
fn migrated(conn: &Connection) -> bool {
    let Ok(value) = conn.query_row(
        "SELECT value FROM kv WHERE key = 'migration.v1-v2'",
        [],
        |row| row.get::<_, String>(0),
    ) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(&value)
        .ok()
        .and_then(|v| v.get("phase").and_then(|p| p.as_str()).map(str::to_string))
        .is_some_and(|phase| phase == "completed")
}

/// The sessions in the database (v2 tables first, then the old rows that the
/// migration has not copied over yet).
fn db_files(agent: &'static str, db: &Path) -> Vec<SessionFile> {
    let Some(conn) = open_read_only(db) else {
        return Vec::new();
    };
    let mut out = db_rows(
        agent,
        db,
        &conn,
        "SELECT s.id, COALESCE(s.parent_id, ''), s.time_updated, COUNT(m.id), COALESCE(MAX(m.time_updated), 0)
         FROM session_v2 s LEFT JOIN session_message m ON m.session_id = s.id GROUP BY s.id",
    );
    let v2 = has_table(&conn, "session_v2");
    let read_old = if v2 {
        // While the migration is under way, an old session not yet copied is
        // read from the old tables; once it has completed, only the new ones.
        has_table(&conn, "session") && !migrated(&conn)
    } else {
        has_table(&conn, "session")
    };
    if read_old {
        let filter = if v2 {
            "WHERE s.id NOT IN (SELECT id FROM session_v2)"
        } else {
            ""
        };
        out.extend(db_rows(
            agent,
            db,
            &conn,
            &format!(
                "SELECT s.id, COALESCE(s.parent_id, ''), s.time_updated, COUNT(m.id), COALESCE(MAX(m.time_updated), 0)
                 FROM session s LEFT JOIN message m ON m.session_id = s.id {filter} GROUP BY s.id"
            ),
        ));
    }
    out
}

/// Run one session-listing query: id, parent, session row's updated time,
/// message count and latest message updated time.
fn db_rows(agent: &'static str, db: &Path, conn: &Connection, query: &str) -> Vec<SessionFile> {
    let Ok(mut rows) = conn.prepare(query) else {
        return Vec::new();
    };
    let listed = rows.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    });
    let mut out = Vec::new();
    if let Ok(listed) = listed {
        for row in listed.flatten() {
            let (id, updated, count, last) = row;
            out.push(SessionFile {
                agent,
                path: db_session_path(db, &id),
                // "Size" is the message count and "modified" the latest
                // change to any of them (magpie openCodeDBFiles precedent).
                size: count.max(0) as u64,
                modified_ms: updated.max(last).max(0),
            });
        }
    }
    out
}

/// The older JSON storage: `storage/session/<project>/<id>.json`, each
/// session's "size" the sum of its message files' sizes and "modified" the
/// latest of their mtimes (a message's file is written again as its reply
/// goes on; magpie openCodeJSONFiles precedent).
fn json_files(agent: &'static str, storage: &Path) -> Vec<SessionFile> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(storage.join("session")) else {
        return out;
    };
    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") || !entry.metadata().is_ok_and(|m| m.is_file()) {
                continue;
            }
            if let Some(file) = json_session_file(agent, &path) {
                out.push(file);
            }
        }
    }
    out
}

/// One JSON-storage session file: id read from the session document (a file
/// that fails to parse is skipped), size/mtime from its message files.
fn json_session_file(agent: &'static str, session_json: &Path) -> Option<SessionFile> {
    let bytes = std::fs::read(session_json).ok()?;
    let info: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let id = info.get("id")?.as_str()?.to_string();
    if id.is_empty() {
        return None;
    }
    // The session's messages live in <storage>/message/<id>/*.json beside
    // the session documents' own tree (<storage>/session/<project>/<id>.json).
    let messages = session_json
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(|storage| storage.join("message").join(&id));
    let mut size = 0u64;
    let mut modified_ms = 0i64;
    if let Some(dir) = messages
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata()
                && meta.is_file()
            {
                size += meta.len();
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                modified_ms = modified_ms.max(mtime);
            }
        }
    }
    Some(SessionFile {
        agent,
        path: session_json.to_path_buf(),
        size,
        modified_ms,
    })
}
