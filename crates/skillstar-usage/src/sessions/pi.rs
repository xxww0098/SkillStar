//! Pi session parsing (shared with omp, whose sessions are the same kind).
//!
//! Pi writes a file per session, `<time>_<session id>.jsonl`, in a folder per
//! working directory under its agent folder's `sessions/` (or all in one
//! folder of the user's choosing): a `"session"` header naming the id, then
//! an entry per line. A session forked from another starts with a copy of
//! that one's entries, times and all; the copy is recognized by the header's
//! `parentSession` (the fork time) and **counted where it was first written**
//! — entries older than the fork are skipped (magpie piParse precedent).
//!
//! Lines are jsonc-tolerated: comments and trailing commas are stripped
//! before parsing (string-aware).
//!
//! ## Token semantics (see the matrix in codex.rs)
//!
//! **pi / omp: `input` already excludes the cache read** — the usage fields
//! map onto [`SessionTokens`] as they stand, the opposite of codex, whose
//! `input_tokens` includes the cached tokens and must be stripped.
//!
//! ## What counts as a call
//!
//! - an assistant message with a model and a usage;
//! - a tool result message whose message-level usage records model work the
//!   tool did (counted on the model in use);
//! - a `usage` entry (omp: `model_usage`) naming its own model;
//! - a `compaction` / `branch_summary` entry, on the model in use.
//!
//! omp's task tool reports its summed usage inside `details`, which is NOT
//! read — the subagents' own sessions already recorded it (magpie evidence).

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::checkpoint;
use super::{FileCheckpoint, SessionCall, SessionFile, SessionParser, SessionTokens};

/// Parser-private version: bump when pi family parsing semantics change.
pub(crate) const PI_PARSER_VERSION: u32 = 1;

/// The latency guard: a gap longer than this between the previous entry and
/// this one is not the call's duration.
const MAX_LATENCY_MS: i64 = 2 * 60 * 60 * 1000;

/// Session parser for the Pi coding agent.
pub(crate) struct PiParser;

impl SessionParser for PiParser {
    const AGENT: &'static str = "pi";
    const PARSER_VERSION: u32 = PI_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        pi_files(Self::AGENT, home)
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        pi_parse(Self::AGENT, Self::PARSER_VERSION, file, prior)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        pi_replay(checkpoint)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Pi's agent folder: `$PI_CODING_AGENT_DIR` (leading `~` expanded), else
/// `<home>/.pi/agent`. The sandbox always wins.
fn pi_agent_dir(home: &Path) -> PathBuf {
    if crate::tool_paths::is_tool_sync_sandboxed() {
        return home.join(".pi").join("agent");
    }
    match std::env::var_os("PI_CODING_AGENT_DIR") {
        Some(dir) if !dir.is_empty() => expand_home(PathBuf::from(dir), home),
        _ => home.join(".pi").join("agent"),
    }
}

/// The one folder the user told Pi to keep its sessions in:
/// `$PI_CODING_AGENT_SESSION_DIR`, else the `sessionDir` setting in the
/// agent folder's settings.json (jsonc-tolerated). Empty for none.
fn pi_session_dir(agent_dir: &Path, home: &Path) -> Option<PathBuf> {
    if !crate::tool_paths::is_tool_sync_sandboxed()
        && let Some(dir) = std::env::var_os("PI_CODING_AGENT_SESSION_DIR")
        && !dir.is_empty()
    {
        return Some(expand_home(PathBuf::from(dir), home));
    }
    let bytes = std::fs::read(agent_dir.join("settings.json")).ok()?;
    let value: serde_json::Value =
        serde_json::from_str(&jsonc_to_json(&String::from_utf8_lossy(&bytes))).ok()?;
    let dir = value.get("sessionDir")?.as_str()?;
    (!dir.is_empty()).then(|| expand_home(PathBuf::from(dir), home))
}

/// Expand a leading `~` / `~/` against `home` (magpie expandHome precedent).
fn expand_home(path: PathBuf, home: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if text == "~" || text.starts_with("~/") || text.starts_with("~\\") {
        return home.join(path.iter().skip(1).collect::<PathBuf>());
    }
    path
}

/// Find pi's session files: `sessions/<folder>/*.jsonl` plus the chosen flat
/// folder, one session file per `<time>_<id>.jsonl` name (magpie piFiles
/// precedent — a folder per working directory, exactly one level deep).
pub(super) fn pi_files(agent: &'static str, home: &Path) -> Vec<SessionFile> {
    let agent_dir = pi_agent_dir(home);
    let mut paths = one_level_jsonl(&agent_dir.join("sessions"));
    if let Some(dir) = pi_session_dir(&agent_dir, home) {
        paths.extend(flat_jsonl(&dir));
    }
    let mut out = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        if let Some(file) = session_file_of(agent, &path) {
            out.push(file);
        }
    }
    out
}

/// `*.jsonl` one level under `root` (the per-working-directory folders).
fn one_level_jsonl(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(folders) = std::fs::read_dir(root) else {
        return out;
    };
    for folder in folders.flatten() {
        if !folder.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(folder.path()) {
            out.extend(
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl")),
            );
        }
    }
    out
}

/// `*.jsonl` directly inside `dir` (the flat folder of the user's choosing).
fn flat_jsonl(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
                .collect()
        })
        .unwrap_or_default()
}

/// One candidate path as a session file when its name says `<time>_<id>`
/// (shared with omp's discovery).
pub(super) fn session_file_of(agent: &'static str, path: &Path) -> Option<SessionFile> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    let (_, id) = stem.split_once('_')?;
    if id.is_empty() {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(SessionFile {
        agent,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms,
    })
}

/// The session a pi-family path belongs to, and whether it is a main session
/// file. omp keeps its subagents' and advisor's sessions in an artifacts
/// folder beside the main file, named `<time>_<parent id>` — files under one
/// of those belong to the parent session (walking up to the first
/// `<time>_<id>`-shaped directory; pi timestamps contain `T` and end in
/// `Z`). Main files carry their own id from their own name.
pub(super) fn family_session_of_path(path: &Path) -> (String, bool) {
    let mut dir = path.parent();
    while let Some(current) = dir {
        if let Some(name) = current.file_name().and_then(|n| n.to_str())
            && let Some((time, id)) = name.split_once('_')
            && time.contains('T')
            && time.ends_with('Z')
            && !id.is_empty()
        {
            return (id.to_string(), false);
        }
        dir = current.parent();
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    match stem.split_once('_') {
        Some((_, id)) if !id.is_empty() => (id.to_string(), true),
        _ => (String::new(), true),
    }
}

// ---------------------------------------------------------------------------
// Line shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct PiLine {
    #[serde(rename = "type", default)]
    line_type: NullStr,
    #[serde(default)]
    id: NullStr,
    #[serde(rename = "parentSession", default)]
    parent_session: NullStr,
    #[serde(rename = "modelId", default)]
    model_id: NullStr,
    #[serde(default)]
    model: NullStr,
    #[serde(default)]
    usage: Option<PiUsage>,
    #[serde(default)]
    message: Option<PiMessage>,
}

#[derive(Deserialize)]
struct PiMessage {
    #[serde(default)]
    role: NullStr,
    #[serde(default)]
    model: NullStr,
    #[serde(default)]
    usage: Option<PiUsage>,
}

/// A string field that reads JSON null as empty, the way pi's own Go
/// unmarshaler does (magpie piLine precedent: `parentSession: null` and the
/// like are everyday shapes).
#[derive(Default)]
struct NullStr(String);

impl NullStr {
    fn as_str(&self) -> &str {
        &self.0
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'de> Deserialize<'de> for NullStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = NullStr;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or null")
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(NullStr(value.to_string()))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(NullStr(String::new()))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(NullStr(String::new()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Pi's own usage tuple: **input without the cache read** (see the module
/// matrix), output as pi reports it.
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
struct PiUsage {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(rename = "cacheRead", default)]
    cache_read: u64,
    #[serde(rename = "cacheWrite", default)]
    cache_write: u64,
}

impl PiUsage {
    fn tokens(self) -> SessionTokens {
        SessionTokens {
            input: self.input,
            output: self.output,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser state and shared parse
// ---------------------------------------------------------------------------

/// The pi family's checkpoint-private state.
#[derive(Default)]
struct PiState {
    session: String,
    header_seen: bool,
    /// The fork cutoff: entries older than this are the copy of the session
    /// forked from, skipped (0 = none).
    since_ms: i64,
    /// The model in use (model_change / the last assistant message).
    model: String,
    /// The last non-copied line's time (the latency baseline).
    last_ms: i64,
    /// End offset of the last call's line (default `from` for new calls).
    last_end: u64,
    calls: Vec<SessionCall>,
}

impl PiState {
    fn from_checkpoint(checkpoint: &FileCheckpoint) -> Self {
        #[derive(Deserialize, Default)]
        struct Stored {
            #[serde(default)]
            session: String,
            #[serde(default)]
            header_seen: bool,
            #[serde(default)]
            since_ms: i64,
            #[serde(default)]
            model: String,
            #[serde(default)]
            last_ms: i64,
            #[serde(default)]
            last_end: u64,
            #[serde(default)]
            calls: Vec<SessionCall>,
        }
        let stored: Stored = serde_json::from_value(checkpoint.agent_state.clone())
            .unwrap_or_default();
        Self {
            session: stored.session,
            header_seen: stored.header_seen,
            since_ms: stored.since_ms,
            model: stored.model,
            last_ms: stored.last_ms,
            last_end: stored.last_end,
            calls: stored.calls,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "session": self.session,
            "header_seen": self.header_seen,
            "since_ms": self.since_ms,
            "model": self.model,
            "last_ms": self.last_ms,
            "last_end": self.last_end,
            "calls": self.calls,
        })
    }
}

/// The pi family's full call view restored from a checkpoint. Entry ids are
/// per-session sequences that repeat across files, so they carry no dedup
/// key (the fork copy is skipped by time instead; magpie precedent).
pub(super) fn pi_replay(checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
    PiState::from_checkpoint(checkpoint)
        .calls
        .into_iter()
        .map(|call| (String::new(), call))
        .collect()
}

/// The pi family's incremental parsing (pi and omp share it; `agent` names
/// the family the file belongs to).
pub(super) fn pi_parse(
    agent: &'static str,
    parser_version: u32,
    file: &SessionFile,
    prior: Option<FileCheckpoint>,
) -> (Vec<SessionCall>, FileCheckpoint) {
    let head = checkpoint::read_head(&file.path);
    if let Some(prior) = &prior
        && prior.version == parser_version
        && prior.size == file.size
        && checkpoint::head_matches(&head, &prior.head_hash)
    {
        return (Vec::new(), prior.clone());
    }
    let (mut state, start_offset) = match prior.as_ref() {
        Some(prior)
            if checkpoint::can_resume(
                prior,
                parser_version,
                file.size,
                &head,
                &checkpoint::prefix_hash(&file.path, prior.size),
            ) =>
        {
            (PiState::from_checkpoint(prior), prior.offset)
        }
        _ => (PiState::default(), 0),
    };
    let (path_session, is_main) = family_session_of_path(&file.path);
    if state.session.is_empty() {
        // The header's id (a main file) or the path-derived one (an omp
        // artifacts file already carries the parent session's id).
        state.session = path_session;
    }

    let mut delta = Vec::new();
    let mut offset = start_offset;
    if let Ok(reader) = std::fs::File::open(&file.path).map(|f| {
        let mut f = f;
        let _ = f.seek(SeekFrom::Start(start_offset));
        BufReader::with_capacity(1 << 20, f)
    }) {
        let mut reader = reader;
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            let Ok(n) = reader.read_until(b'\n', &mut buffer) else { break };
            if n == 0 {
                break;
            }
            if !buffer.ends_with(b"\n") {
                break; // A half-written line: not consumed
            }
            let line_end = offset + n as u64;
            let trimmed: &[u8] = match buffer.last() {
                Some(b'\r') | Some(b'\n') => &buffer[..buffer.len() - 1],
                _ => &buffer[..],
            };
            pi_line(agent, &mut state, is_main, &mut delta, trimmed, &file.path, line_end);
            offset = line_end;
        }
    }

    let checkpoint = FileCheckpoint {
        version: parser_version,
        head_hash: checkpoint::head_hash(&head),
        prefix_hash: checkpoint::prefix_hash(&file.path, file.size),
        size: file.size,
        offset,
        agent_state: state.to_json(),
        calls_seen: state.calls.len() as u64,
    };
    (delta, checkpoint)
}

/// Handle one entry line. Lines older than the fork cutoff (the header's
/// parentSession time) are the copy of the session forked from and are
/// skipped without touching the state.
fn pi_line(
    agent: &'static str,
    state: &mut PiState,
    is_main: bool,
    delta: &mut Vec<SessionCall>,
    line: &[u8],
    file: &Path,
    line_end: u64,
) {
    let Some(text) = std::str::from_utf8(line).ok() else { return };
    let at = entry_timestamp(text);
    let copied = state.since_ms > 0 && at > 0 && at < state.since_ms;

    let cleaned = jsonc_to_json(text);
    let Ok(parsed) = serde_json::from_str::<PiLine>(&cleaned) else {
        if !copied {
            note_line_time(state, at);
        }
        return;
    };
    match parsed.line_type.as_str() {
        "session" if !state.header_seen => {
            // The header sits at the fork time itself, never copied.
            state.header_seen = true;
            if is_main && !parsed.id.is_empty() {
                state.session = parsed.id.0.clone();
            }
            if !parsed.parent_session.is_empty() {
                state.since_ms = at;
            }
        }
        // The model in force is noted even from a fork's copied entries
        // (magpie precedent): the fork's own later entries rely on it.
        "model_change" => {
            if !parsed.model_id.is_empty() {
                state.model = parsed.model_id.0.clone();
            } else if let Some((_, id)) = parsed.model.as_str().split_once('/')
                && !id.is_empty()
            {
                state.model = id.to_string();
            }
        }
        // The fork's copy of everything else: counted where it was first
        // written, so it is skipped here without touching the state.
        _ if copied => return,
        "usage" | "model_usage" => {
            let usage = parsed.usage.unwrap_or_default();
            pi_emit(agent, state, delta, at, parsed.model.as_str(), usage, file, line_end);
        }
        "compaction" | "branch_summary" => {
            let usage = parsed.usage.unwrap_or_default();
            let model = state.model.clone();
            pi_emit(agent, state, delta, at, &model, usage, file, line_end);
        }
        "message" => {
            let Some(message) = &parsed.message else {
                note_line_time(state, at);
                return;
            };
            match message.role.as_str() {
                "assistant" => {
                    if !message.model.is_empty() {
                        state.model = message.model.0.clone();
                    }
                    let usage = message.usage.unwrap_or_default();
                    let model = message.model.0.clone();
                    pi_emit(agent, state, delta, at, &model, usage, file, line_end);
                }
                "user" => {}
                // A tool result may carry the usage of model work the tool
                // did, on the model in use. omp's task tool reports its
                // summed usage inside `details`, which is not read — the
                // subagents' own sessions already recorded it.
                _ => {
                    let usage = message.usage.unwrap_or_default();
                    let model = state.model.clone();
                    pi_emit(agent, state, delta, at, &model, usage, file, line_end);
                }
            }
        }
        _ => {}
    }
    note_line_time(state, at);
}

/// Note the line's time as the latency baseline for the calls after it.
fn note_line_time(state: &mut PiState, at: i64) {
    if at > 0 {
        state.last_ms = at;
    }
}

/// Record one model call (into both the full view and this run's delta); a
/// model-less or all-zero usage line is not a call.
#[allow(clippy::too_many_arguments)]
fn pi_emit(
    agent: &'static str,
    state: &mut PiState,
    delta: &mut Vec<SessionCall>,
    at: i64,
    model: &str,
    usage: PiUsage,
    file: &Path,
    line_end: u64,
) {
    let tokens = usage.tokens();
    if model.is_empty() || tokens.is_zero() || at == 0 {
        return;
    }
    // Measured from the previous entry (the prompt that asked, the tool
    // result that came back); the line's own time is noted afterwards.
    let latency_ms = (state.last_ms > 0 && at > state.last_ms && at - state.last_ms < MAX_LATENCY_MS)
        .then_some((at - state.last_ms) as u64);
    let call = SessionCall {
        at,
        agent: agent.to_string(),
        session: state.session.clone(),
        model_asked: model.to_string(),
        model_answered: model.to_string(),
        tokens,
        effort: None,
        request_id: None,
        error_kind: None,
        latency_ms,
        file: file.to_path_buf(),
        from: state.last_end,
        to: line_end,
    };
    state.last_end = line_end;
    state.calls.push(call.clone());
    delta.push(call);
}

/// The entry's `"timestamp":"…"` value as epoch milliseconds (first
/// occurrence — pi entries open with it; values longer than 40 bytes count
/// as no timestamp).
fn entry_timestamp(line: &str) -> i64 {
    const MARK: &str = r#""timestamp":""#;
    let Some(start) = line.find(MARK) else { return 0 };
    let rest = &line[start + MARK.len()..];
    let end = rest.find('"').unwrap_or(0);
    if end == 0 || end > 40 {
        return 0;
    }
    chrono::DateTime::parse_from_rfc3339(&rest[..end])
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

/// Strip jsonc comments (`//` and `/* */`) and trailing commas, both
/// string-aware; pi tolerates them in its entries and its settings.json
/// (magpie tidwall/jsonc precedent). Line comments end at the newline (the
/// whole file may be passed); an unterminated block comment ends with the
/// input.
fn jsonc_to_json(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                // Line comment: swallow up to (and keeping) the newline.
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => out.push(ch),
        }
    }
    strip_trailing_commas(&out)
}

/// Remove commas whose next non-whitespace character is `}` or `]`
/// (string-aware).
fn strip_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if ch == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 1;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut look = index + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && (chars[look] == '}' || chars[look] == ']') {
                index += 1; // drop the trailing comma
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
}
