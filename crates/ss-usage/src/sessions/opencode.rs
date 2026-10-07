//! OpenCode session parsing (SQLite database, with the older JSON-file
//! storage as the fallback).
//!
//! OpenCode keeps a session as rows, not a file of lines: an assistant
//! message row carries its model and its tokens — **input without the cache
//! and output with the reasoning** (see the token matrix in codex.rs; the
//! opencode fields map onto [`SessionTokens`] as they stand, reasoning added
//! into output). A subagent's session is a session of its own whose parent is
//! the session it ran in, and its calls are attributed to the root session
//! (magpie ocRoots precedent).
//!
//! Because a message row is **rewritten in place** as its reply goes on,
//! there is no reading on from before: every run reads the session whole
//! (magpie parseOpenCode precedent). The checkpoint machinery therefore only
//! records the view; `from`/`to` of an opencode call are 0/0 (its text lives
//! in the database, not at a byte interval) and the returned delta is the
//! whole refreshed view.
//!
//! ## Stores
//!
//! - database: path `<db>#<session id>`; OpenCode 2's `session_message` rows
//!   when the session has been copied into `session_v2`, else the old
//!   `message` rows (discovery has already excluded the copied-over ids —
//!   see opencode_discovery.rs);
//! - JSON storage: the session document under `storage/session/<project>/`,
//!   its messages `storage/message/<sid>/*.json` in name order.

use std::path::Path;

use serde::Deserialize;

use super::checkpoint;
use super::opencode_discovery::{open_read_only, split_db_path};
use super::{FileCheckpoint, SessionCall, SessionFile, SessionParser, SessionTokens};

/// Parser-private version: bump when opencode parsing semantics change.
pub(crate) const OPENCODE_PARSER_VERSION: u32 = 1;

/// Session parser for OpenCode.
pub(crate) struct OpenCodeParser;

impl SessionParser for OpenCodeParser {
    const AGENT: &'static str = "opencode";
    const PARSER_VERSION: u32 = OPENCODE_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        super::opencode_discovery::opencode_files(Self::AGENT, home)
    }

    fn parse(
        &self,
        file: &SessionFile,
        _prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        let calls = split_db_path(&file.path)
            .and_then(|(db, sid)| db_session_calls(&db, &sid))
            .unwrap_or_else(|| json_session_calls(&file.path));

        let head = checkpoint::read_head(&file.path);
        let checkpoint = FileCheckpoint {
            version: Self::PARSER_VERSION,
            head_hash: checkpoint::head_hash(&head),
            prefix_hash: checkpoint::prefix_hash(&file.path, file.size),
            size: file.size,
            // Rewritten in place: always reread whole; the "offset" just
            // marks the current view as consumed.
            offset: file.size,
            agent_state: serde_json::json!({ "calls": calls }),
            calls_seen: calls.len() as u64,
        };
        (calls.clone(), checkpoint)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        // OpenCode message ids are per-session sequences in the wild; they
        // do not participate in cross-file dedup (magpie precedent: only the
        // claude family dedups by message id).
        #[derive(Deserialize, Default)]
        struct Stored {
            #[serde(default)]
            calls: Vec<SessionCall>,
        }
        let stored: Stored =
            serde_json::from_value(checkpoint.agent_state.clone()).unwrap_or_default();
        stored
            .calls
            .into_iter()
            .map(|call| (String::new(), call))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Message documents
// ---------------------------------------------------------------------------

/// One message, whichever store it came from.
struct OcDoc {
    role: String,
    model: String,
    tokens: Option<OcTokens>,
    created: i64,
    completed: i64,
    id: String,
}

/// The usage tuple both stores share (opencode's own terms: input without
/// the cache, reasoning on top of output).
#[derive(Deserialize, Clone, Copy)]
struct OcTokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    reasoning: u64,
    #[serde(default)]
    cache: OcCache,
}

#[derive(Deserialize, Clone, Copy, Default)]
struct OcCache {
    #[serde(default)]
    read: u64,
    #[serde(default)]
    write: u64,
}

#[derive(Deserialize, Default)]
struct OcTime {
    #[serde(default)]
    created: i64,
    #[serde(default)]
    completed: i64,
}

/// An old `message` table row's JSON document. `modelID` is matched in any
/// case by OpenCode's own readers, and real files spell it both ways.
#[derive(Deserialize)]
struct V1Doc {
    #[serde(default)]
    id: String,
    #[serde(default)]
    role: String,
    #[serde(default, alias = "modelID", alias = "modelId")]
    model_id: String,
    #[serde(default)]
    tokens: Option<OcTokens>,
    #[serde(default)]
    time: OcTime,
}

impl From<V1Doc> for OcDoc {
    fn from(doc: V1Doc) -> Self {
        OcDoc {
            role: doc.role,
            model: doc.model_id,
            tokens: doc.tokens,
            created: doc.time.created,
            completed: doc.time.completed,
            id: doc.id,
        }
    }
}

/// An OpenCode 2 `session_message` row's `data` document: a user's data is
/// its prompt (no tokens of its own); an assistant's has model {id},
/// tokens and time; a compaction's has its own tokens and names no model —
/// it counts on the one last replied with (magpie ocV2DB precedent).
#[derive(Deserialize)]
struct V2Data {
    #[serde(default)]
    model: Option<V2Model>,
    #[serde(default)]
    tokens: Option<OcTokens>,
    #[serde(default)]
    time: OcTime,
}

#[derive(Deserialize)]
struct V2Model {
    #[serde(default)]
    id: String,
}

// ---------------------------------------------------------------------------
// The database store
// ---------------------------------------------------------------------------

/// One database session's calls: v2 rows when the session was copied into
/// `session_v2`, else the old `message` rows, attributed to the root session.
fn db_session_calls(db: &Path, sid: &str) -> Option<Vec<SessionCall>> {
    let conn = open_read_only(db)?;
    let root = db_root_session(&conn, sid);
    let docs = if in_session_v2(&conn, sid) {
        v2_docs(&conn, sid)
    } else {
        v1_docs(&conn, sid)
    };
    Some(docs_to_calls(&root, db, docs))
}

fn in_session_v2(conn: &rusqlite::Connection, sid: &str) -> bool {
    conn.query_row("SELECT 1 FROM session_v2 WHERE id = ?1", [sid], |_| Ok(()))
        .is_ok()
}

/// The session this one ran in, following parent links to the top (a
/// subagent's subagent counts there too; magpie ocRoots precedent).
fn db_root_session(conn: &rusqlite::Connection, sid: &str) -> String {
    let mut root = sid.to_string();
    for _ in 0..64 {
        let parent = ["session_v2", "session"]
            .iter()
            .find_map(|table| {
                conn.query_row(
                    &format!("SELECT COALESCE(parent_id, '') FROM {table} WHERE id = ?1"),
                    [&root],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            })
            .unwrap_or_default();
        if parent.is_empty() {
            break;
        }
        root = parent;
    }
    root
}

/// The old tables' messages, sorted by creation time then id (the table
/// itself has no order; magpie parseOpenCode sorts).
fn v1_docs(conn: &rusqlite::Connection, sid: &str) -> Vec<OcDoc> {
    let Ok(mut rows) = conn.prepare("SELECT data FROM message WHERE session_id = ?1") else {
        return Vec::new();
    };
    let mut docs: Vec<OcDoc> = rows
        .query_map([sid], |row| row.get::<_, String>(0))
        .map(|listed| {
            listed
                .flatten()
                .filter_map(|data| serde_json::from_str::<V1Doc>(&data).ok().map(OcDoc::from))
                .collect()
        })
        .unwrap_or_default();
    docs.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    docs
}

/// OpenCode 2's rows: only prompts, replies and compactions, in `seq` order;
/// a compaction takes the model last replied with.
fn v2_docs(conn: &rusqlite::Connection, sid: &str) -> Vec<OcDoc> {
    let Ok(mut rows) = conn.prepare(
        "SELECT id, type, data FROM session_message
         WHERE session_id = ?1 AND type IN ('user', 'assistant', 'compaction') ORDER BY seq",
    ) else {
        return Vec::new();
    };
    let listed = rows.query_map([sid], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    });
    let mut out = Vec::new();
    let mut last_model = String::new();
    if let Ok(listed) = listed {
        for (id, kind, data) in listed.flatten() {
            let Ok(parsed) = serde_json::from_str::<V2Data>(&data) else {
                continue;
            };
            let model = parsed
                .model
                .as_ref()
                .map(|m| m.id.clone())
                .unwrap_or_default();
            let doc = match kind.as_str() {
                "user" => OcDoc {
                    id,
                    role: "user".to_string(),
                    model: String::new(),
                    tokens: None,
                    created: parsed.time.created,
                    completed: parsed.time.completed,
                },
                "assistant" => {
                    if !model.is_empty() {
                        last_model = model.clone();
                    }
                    OcDoc {
                        id,
                        role: "assistant".to_string(),
                        model,
                        tokens: parsed.tokens,
                        created: parsed.time.created,
                        completed: parsed.time.completed,
                    }
                }
                // A compaction names no model: it counts on the one last
                // replied with.
                _ => OcDoc {
                    id,
                    role: "assistant".to_string(),
                    model: last_model.clone(),
                    tokens: parsed.tokens,
                    created: parsed.time.created,
                    completed: parsed.time.completed,
                },
            };
            out.push(doc);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The JSON storage fallback
// ---------------------------------------------------------------------------

/// One JSON-storage session's calls. The session document names the id (its
/// file name is the fallback); messages are read in file-name order.
fn json_session_calls(session_json: &Path) -> Vec<SessionCall> {
    // <storage>/session/<project>/<id>.json
    let Some(storage) = session_json
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    else {
        return Vec::new();
    };
    let sid = session_id_of_json(session_json).unwrap_or_else(|| {
        session_json
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let root = json_root_session(storage, &sid);
    let message_dir = storage.join("message").join(&sid);
    let mut docs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&message_dir) {
        let mut paths: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        for path in paths {
            if let Ok(bytes) = std::fs::read(&path)
                && let Ok(doc) = serde_json::from_slice::<V1Doc>(&bytes)
            {
                docs.push(OcDoc::from(doc));
            }
        }
    }
    docs.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    docs_to_calls(&root, session_json, docs)
}

/// The session document's `id` field.
fn session_id_of_json(session_json: &Path) -> Option<String> {
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(session_json).ok()?).ok()?;
    let id = value.get("id")?.as_str()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// Follow `parentID` links through the session documents to the root session.
fn json_root_session(storage: &Path, sid: &str) -> String {
    let mut root = sid.to_string();
    for _ in 0..64 {
        let Some(parent) = json_parent_of(storage, &root) else {
            break;
        };
        if parent.is_empty() {
            break;
        }
        root = parent;
    }
    root
}

/// A session's `parentID` from its document under `storage/session/*/<id>.json`.
fn json_parent_of(storage: &Path, sid: &str) -> Option<String> {
    let session_dir = storage.join("session");
    let projects = std::fs::read_dir(session_dir).ok()?;
    for project in projects.flatten() {
        let candidate = project.path().join(format!("{sid}.json"));
        if !candidate.is_file() {
            continue;
        }
        let Ok(value) =
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&candidate).ok()?)
        else {
            continue;
        };
        return value
            .get("parentID")
            .and_then(|p| p.as_str())
            .map(str::to_string);
    }
    None
}

// ---------------------------------------------------------------------------
// Documents to calls
// ---------------------------------------------------------------------------

/// Turn one session's messages into calls: an assistant message with a model
/// and a usage tuple (compactions included); zero usage is not a call.
fn docs_to_calls(root_session: &str, file: &Path, docs: Vec<OcDoc>) -> Vec<SessionCall> {
    let mut out = Vec::new();
    for doc in docs {
        if doc.role != "assistant" || doc.model.is_empty() {
            continue;
        }
        let Some(usage) = doc.tokens else { continue };
        let tokens = SessionTokens {
            // Input as it stands (already without the cache); the reasoning
            // tokens ride inside output (magpie parseOpenCode precedent).
            input: usage.input,
            output: usage.output.saturating_add(usage.reasoning),
            cache_read: usage.cache.read,
            cache_write: usage.cache.write,
        };
        if tokens.is_zero() {
            continue;
        }
        let at = if doc.completed > 0 {
            doc.completed
        } else {
            doc.created
        };
        let latency_ms =
            (doc.completed > doc.created).then_some((doc.completed - doc.created) as u64);
        out.push(SessionCall {
            at,
            agent: OpenCodeParser::AGENT.to_string(),
            session: root_session.to_string(),
            model_asked: doc.model.clone(),
            model_answered: doc.model,
            tokens,
            effort: None,
            request_id: None,
            error_kind: None,
            latency_ms,
            // The text lives in the store, not at a byte interval; the path
            // still names where the session is kept (the pseudo `db#id`
            // path or the session document).
            file: file.to_path_buf(),
            from: 0,
            to: 0,
        });
    }
    out
}
