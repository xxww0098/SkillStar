//! Claude family session parsing: Claude Code and Claude Desktop (Code tab /
//! Cowork).
//!
//! Both families' session files use Claude Code's projects JSONL layout
//! (`<config>/projects/<project>/<id>.jsonl`; sub agents in
//! `<id>/subagents/*.jsonl`) and share [`claude_parse`]; they differ only in
//! discovery:
//!
//! - **claude-code**: `projects` under `$CLAUDE_CONFIG_DIR` (default
//!   `~/.claude`). The `SKILLSTAR_TOOL_SYNC_HOME` sandbox always wins
//!   (tool_paths.rs precedent: tests never escape into a real `~/.claude`,
//!   even when `CLAUDE_CONFIG_DIR` is exported).
//! - **claude-desktop**: the Cowork sessions' own `.claude` homes under the
//!   Desktop data directory (macOS `~/Library/Application
//!   Support/{Claude,Claude-3p}`):
//!   `local-agent-mode-sessions/*/*/local_*/.claude/projects` (magpie
//!   calls.go precedent). Additionally, session files produced by the
//!   Claude Code embedded in Desktop are written into the ordinary claude
//!   projects tree but tagged `claude-desktop` by the inline `entrypoint`
//!   (the value observed on this machine is `claude-desktop-3p`);
//!   [`claude_parse`] attributes them to `claude-desktop` by prefix,
//!   independent of directory location.
//!
//! ## Line shapes (magpie claude.go / calls.go evidence + local sampling)
//!
//! One event per line. An assistant line is "one block of one message";
//! later blocks of the same message **re-carry the entire message's usage**
//! (the later block has the final say); user lines are prompts or tool
//! results; attachment lines carry identity (which model identity the agent
//! is currently running as). All parsing rules are pinned by tests (see
//! claude_tests.rs):
//!
//! 1. Same message id, multiple blocks: later block usage overrides the
//!    earlier one, and `from` takes the first block's line position;
//! 2. Incremental resume keeps the same override semantics through the
//!    checkpoint's private state (msgs map / last timestamps / identity);
//! 3. Line-head sniffing: lines without `"type":"assistant"` only get a
//!    cheap scan for identity/user markers, no full-line JSON decode (most
//!    bytes of a session file are tool results and attachment bodies).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::checkpoint;
use super::claude_discovery::{
    cc_files, claude_code_config_dir, cowork_claude_homes, desktop_data_dirs, session_of_path,
};
use super::{FileCheckpoint, SessionCall, SessionFile, SessionParser, SessionTokens};

/// Parser-private version: bump when claude family parsing semantics change
/// (old checkpoints reread in full).
pub(crate) const CLAUDE_PARSER_VERSION: u32 = 1;

/// Line-sniffing substrings (a whole-line bytes::contains is enough to
/// decide; quotes inside the keys would be escaped inside string values and
/// cannot hit body text by mistake).
const ASSISTANT_MARK: &[u8] = br#""type":"assistant""#;
const USER_MARK: &[u8] = br#""type":"user""#;
const IDENTITY_MARK: &[u8] = br#""identity":{"modelId":""#;
const TIMESTAMP_MARK: &[u8] = br#""timestamp":""#;
const SYNTHETIC_MODEL: &str = "<synthetic>";

// ---------------------------------------------------------------------------
// Parser instances: same parse, different discovery / AGENT
// ---------------------------------------------------------------------------

/// Session parser for the Claude Code CLI.
pub(crate) struct ClaudeCodeParser;

impl SessionParser for ClaudeCodeParser {
    const AGENT: &'static str = "claude-code";
    const PARSER_VERSION: u32 = CLAUDE_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        cc_files(Self::AGENT, &claude_code_config_dir(home).join("projects"))
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        claude_parse(Self::AGENT, Self::PARSER_VERSION, file, prior)
    }

    fn unchanged(&self, file: &SessionFile, prior: &FileCheckpoint) -> bool {
        checkpoint::is_unchanged(prior, file, Self::PARSER_VERSION)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        claude_replay(checkpoint)
    }
}

/// Session parser for Claude Desktop (Code tab / Cowork).
pub(crate) struct ClaudeDesktopParser;

impl SessionParser for ClaudeDesktopParser {
    const AGENT: &'static str = "claude-desktop";
    const PARSER_VERSION: u32 = CLAUDE_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        let mut out = Vec::new();
        for dir in desktop_data_dirs(home) {
            // Cowork gives every session its own Claude Code home (the glob
            // from magpie calls.go:
            // local-agent-mode-sessions/*/*/local_*/.claude). Local
            // verification (2026-10, that version of Claude Desktop) found
            // no local_* directory at this layer, and Desktop's Code-tab
            // session bodies do not land here — see the module docs;
            // attribution currently relies on the inline entrypoint. The
            // layout is picked up as soon as it appears; it does not block
            // anything.
            for claude_home in cowork_claude_homes(&dir) {
                out.extend(cc_files(Self::AGENT, &claude_home.join("projects")));
            }
        }
        out
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        claude_parse(Self::AGENT, Self::PARSER_VERSION, file, prior)
    }

    fn unchanged(&self, file: &SessionFile, prior: &FileCheckpoint) -> bool {
        checkpoint::is_unchanged(prior, file, Self::PARSER_VERSION)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        claude_replay(checkpoint)
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// One assistant line (everything needed once the content body is dropped).
///
/// String fields use [`LenientStr`]: other shapes (numbers, objects) read as
/// empty strings instead of failing the whole line's parse (magpie ccStr
/// precedent — real files do contain lines with drifted field shapes).
#[derive(Deserialize)]
struct AssistantLine {
    #[serde(rename = "type", default)]
    line_type: LenientStr,
    #[serde(default)]
    timestamp: LenientStr,
    #[serde(rename = "sessionId", default)]
    session_id: LenientStr,
    #[serde(rename = "requestId", default)]
    request_id: LenientStr,
    #[serde(default)]
    entrypoint: LenientStr,
    #[serde(rename = "isApiErrorMessage", default)]
    api_error: bool,
    #[serde(default)]
    error: LenientStr,
    #[serde(default)]
    effort: LenientStr,
    #[serde(rename = "perTurnEffort", default)]
    per_turn_effort: LenientStr,
    #[serde(default)]
    message: AssistantMessage,
}

#[derive(Deserialize, Default)]
struct AssistantMessage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    model: LenientStr,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(rename = "input_tokens", default)]
    input: u64,
    #[serde(rename = "output_tokens", default)]
    output: u64,
    #[serde(rename = "cache_read_input_tokens", default)]
    cache_read: u64,
    #[serde(rename = "cache_creation_input_tokens", default)]
    cache_write: u64,
}

/// The only field needed from an identity attachment line.
#[derive(Deserialize)]
struct IdentityLine {
    attachment: IdentityAttachment,
}

#[derive(Deserialize)]
struct IdentityAttachment {
    identity: IdentityModel,
}

#[derive(Deserialize)]
struct IdentityModel {
    #[serde(rename = "modelId", default)]
    model_id: LenientStr,
}

/// A lenient string where "a string field reading another shape (number,
/// object, null) counts as empty" (magpie ccStr precedent: real session
/// files do contain lines with shape drift).
#[derive(Default)]
struct LenientStr(String);

impl<'de> Deserialize<'de> for LenientStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = LenientStr;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("any JSON value; non-strings read as empty")
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(LenientStr(value.to_string()))
            }
            // Number / boolean / object / array / null: read as empty; do
            // not fail the whole line's parse.
            fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            fn visit_i64<E>(self, _v: i64) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            // map/seq must consume all their content before returning:
            // leaving a half-consumed stream desynchronizes serde_json's
            // parser state and produces baffling trailing-characters errors.
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(LenientStr(String::new()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(LenientStr(String::new()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

impl LenientStr {
    fn as_str(&self) -> &str {
        &self.0
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The claude family's checkpoint-private state
/// (`FileCheckpoint::agent_state`).
#[derive(Default)]
struct ClaudeState {
    /// message id → the message's final call (later blocks override) + the
    /// first block's ask time.
    msgs: BTreeMap<String, MsgEntry>,
    /// Last user line timestamp (latency baseline).
    last_user_ms: i64,
    /// Last assistant line timestamp.
    last_asst_ms: i64,
    /// End offset of the last assistant line (default `from` for new calls).
    asst_end: u64,
    /// The model identity the agent is currently running as, as noted by an
    /// identity attachment (model_asked).
    requested: String,
    /// Count of calls without a message id (counted separately; not part of
    /// override dedup).
    unnamed: u64,
}

#[derive(Clone)]
struct MsgEntry {
    call: SessionCall,
    /// A later block of the same message also measures latency from "when
    /// the ask started", not from the previous block.
    asked_ms: i64,
}

impl ClaudeState {
    fn from_checkpoint(checkpoint: &FileCheckpoint) -> Self {
        #[derive(Deserialize, Default)]
        struct Stored {
            #[serde(default)]
            msgs: BTreeMap<String, StoredMsg>,
            #[serde(default)]
            last_user_ms: i64,
            #[serde(default)]
            last_asst_ms: i64,
            #[serde(default)]
            asst_end: u64,
            #[serde(default)]
            requested: String,
            #[serde(default)]
            unnamed: u64,
        }
        #[derive(Deserialize)]
        struct StoredMsg {
            call: SessionCall,
            asked_ms: i64,
        }
        let stored: Stored =
            serde_json::from_value(checkpoint.agent_state.clone()).unwrap_or_default();
        Self {
            msgs: stored
                .msgs
                .into_iter()
                .map(|(id, m)| {
                    (
                        id,
                        MsgEntry {
                            call: m.call,
                            asked_ms: m.asked_ms,
                        },
                    )
                })
                .collect(),
            last_user_ms: stored.last_user_ms,
            last_asst_ms: stored.last_asst_ms,
            asst_end: stored.asst_end,
            requested: stored.requested,
            unnamed: stored.unnamed,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        #[derive(Serialize)]
        struct StoredMsg<'a> {
            call: &'a SessionCall,
            asked_ms: i64,
        }
        let msgs: serde_json::Map<String, serde_json::Value> = self
            .msgs
            .iter()
            .map(|(id, entry)| {
                (
                    id.clone(),
                    serde_json::to_value(StoredMsg {
                        call: &entry.call,
                        asked_ms: entry.asked_ms,
                    })
                    .unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        serde_json::json!({
            "msgs": msgs,
            "last_user_ms": self.last_user_ms,
            "last_asst_ms": self.last_asst_ms,
            "asst_end": self.asst_end,
            "requested": self.requested,
            "unnamed": self.unnamed,
        })
    }
}

// Serialize is used by ClaudeState::to_json's msgs struct (it embeds SessionCall).
/// The full call view restored from a checkpoint (an empty msg id means no
/// dedup key).
fn claude_replay(checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
    ClaudeState::from_checkpoint(checkpoint)
        .msgs
        .into_iter()
        .map(|(msg, entry)| (msg, entry.call))
        .collect()
}

/// Claude family incremental parsing (shared by both parser instances;
/// `agent` is the family the file belongs to).
fn claude_parse(
    agent: &'static str,
    parser_version: u32,
    file: &SessionFile,
    prior: Option<FileCheckpoint>,
) -> (Vec<SessionCall>, FileCheckpoint) {
    let head = checkpoint::read_head(&file.path);
    // Not grown and head fingerprint matches: do not open the body, hand the
    // checkpoint back unchanged. A same-length replacement is caught here by
    // the head check (head changed → full prefix check → reread from zero).
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
            (ClaudeState::from_checkpoint(prior), prior.offset)
        }
        _ => (ClaudeState::default(), 0),
    };

    let mut delta = Vec::new();
    let mut offset = start_offset;
    if let Ok(mut reader) = std::fs::File::open(&file.path).map(|f| {
        let mut f = f;
        let _ = f.seek(SeekFrom::Start(start_offset));
        BufReader::with_capacity(1 << 20, f)
    }) {
        loop {
            let mut line = Vec::new();
            let Ok(n) = reader.read_until(b'\n', &mut line) else {
                break;
            };
            if n == 0 {
                break; // EOF
            }
            if !line.ends_with(b"\n") {
                break; // A half-written line: not consumed, offset not advanced
            }
            let line_end = offset + n as u64;
            let trimmed: &[u8] = match line.last() {
                Some(b'\r') | Some(b'\n') => &line[..line.len() - 1],
                _ => &line[..],
            };
            claude_line(agent, &mut state, &mut delta, trimmed, &file.path, line_end);
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
        calls_seen: state.msgs.len() as u64 + state.unnamed,
    };
    (dedup_delta(delta), checkpoint)
}

/// Make this run's delta itself idempotent by message id: of the multiple
/// blocks of one message only the **final version** survives, positioned at
/// its first occurrence (consistent with From taking the first line). Calls
/// without a message id (e.g. error lines) do not participate in dedup.
/// Consumers can accumulate deltas directly without double counting.
fn dedup_delta(delta: Vec<(String, SessionCall)>) -> Vec<SessionCall> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<SessionCall> = Vec::with_capacity(delta.len());
    for (id, call) in delta {
        if id.is_empty() {
            out.push(call);
            continue;
        }
        match seen.get(&id) {
            Some(&index) => out[index] = call, // Overriding block replaces; keeps first-occurrence position
            None => {
                seen.insert(id, out.len());
                out.push(call);
            }
        }
    }
    out
}

/// Handle one line: sniff first, decode the full line only when it may be an
/// assistant line. delta is (message id, call); calls with an empty id do
/// not participate in cross-file/cross-block dedup.
fn claude_line(
    agent: &'static str,
    state: &mut ClaudeState,
    delta: &mut Vec<(String, SessionCall)>,
    line: &[u8],
    file: &Path,
    line_end: u64,
) {
    if !has_substring(line, ASSISTANT_MARK) {
        // Sniffing fast path: the bulk of a session file (tool results,
        // attachment bodies) only gets a cheap scan for two markers, no JSON
        // decode.
        if has_substring(line, IDENTITY_MARK) {
            if let Ok(identity) = serde_json::from_slice::<IdentityLine>(line)
                && !identity.attachment.identity.model_id.is_empty()
            {
                state.requested = identity.attachment.identity.model_id.as_str().to_string();
            }
        } else if has_substring(line, USER_MARK)
            && let Some(at) = timestamp_bytes(line)
        {
            state.last_user_ms = at;
        }
        return;
    }

    let Ok(parsed) = serde_json::from_slice::<AssistantLine>(line) else {
        return;
    };
    if parsed.line_type.as_str() != "assistant" {
        return;
    }
    let at = timestamp_bytes(line).unwrap_or_else(|| parse_rfc3339_ms(parsed.timestamp.as_str()));
    if at == 0 {
        return;
    }
    let model = parsed.message.model.as_str();
    // Synthetic lines are Claude Code's own annotations; only API error
    // lines count as a call.
    if model == SYNTHETIC_MODEL && !parsed.api_error {
        return;
    }

    let mut call = SessionCall {
        at,
        agent: attributed_agent(agent, parsed.entrypoint.as_str()).to_string(),
        session: if parsed.session_id.is_empty() {
            session_of_path(file)
        } else {
            parsed.session_id.as_str().to_string()
        },
        model_asked: String::new(),
        model_answered: String::new(),
        tokens: SessionTokens::default(),
        effort: if !parsed.per_turn_effort.is_empty() {
            Some(parsed.per_turn_effort.as_str().to_string())
        } else if !parsed.effort.is_empty() {
            Some(parsed.effort.as_str().to_string())
        } else {
            None
        },
        request_id: if parsed.request_id.is_empty() {
            None
        } else {
            Some(parsed.request_id.as_str().to_string())
        },
        error_kind: None,
        latency_ms: None,
        file: file.to_path_buf(),
        from: state.asst_end,
        to: line_end,
    };

    if parsed.api_error {
        call.error_kind = Some(if parsed.error.is_empty() {
            "unknown".to_string()
        } else {
            parsed.error.as_str().to_string()
        });
    } else {
        let Some(usage) = &parsed.message.usage else {
            return; // An assistant line without usage is not a call (magpie precedent)
        };
        call.tokens = SessionTokens {
            input: usage.input,
            output: usage.output,
            cache_read: usage.cache_read,
            cache_write: usage.cache_write,
        };
        if call.tokens.is_zero() {
            return;
        }
        call.model_answered = model.to_string();
    }
    call.model_asked = if state.requested.is_empty() {
        call.model_answered.clone()
    } else {
        state.requested.clone()
    };

    // latency: measured from "when this ask started" — a later block of the
    // same message uses the ask time recorded by its first block, otherwise
    // the last user line; if neither is earlier than the last assistant
    // line, the latter is used.
    let id = parsed.message.id.as_str();
    let (asked_ms, prior) = match state.msgs.get(id) {
        Some(entry) if !id.is_empty() => (entry.asked_ms, Some(entry.call.from)),
        _ => {
            let asked = state.last_asst_ms.max(state.last_user_ms);
            (asked, None)
        }
    };
    if asked_ms > 0 && at > asked_ms {
        call.latency_ms = Some((at - asked_ms) as u64);
    }
    if let Some(first_from) = prior {
        call.from = first_from; // Later block of the same message: From takes the first line
    }

    if id.is_empty() {
        // Calls without a message id (e.g. synthetic error lines) do not
        // participate in override/dedup; counted separately.
        state.unnamed += 1;
    } else {
        state.msgs.insert(
            id.to_string(),
            MsgEntry {
                call: call.clone(),
                asked_ms,
            },
        );
    }
    delta.push((id.to_string(), call));
    state.last_asst_ms = at;
    state.asst_end = line_end;
}

/// An inline entrypoint identifying Claude Code embedded in Desktop: the
/// `claude-desktop` prefix (the value observed on this machine is
/// `claude-desktop-3p`; magpie recorded `claude-desktop`). Attribution
/// applies only to the claude-code family (the desktop family already is
/// desktop).
fn attributed_agent(agent: &'static str, entrypoint: &str) -> &'static str {
    if agent == ClaudeCodeParser::AGENT && entrypoint.starts_with("claude-desktop") {
        ClaudeDesktopParser::AGENT
    } else {
        agent
    }
}

/// Byte substring test (`slice::contains` only finds single elements; the
/// markers here are multi-byte). Quotes inside a marker key, should they
/// appear in a string value, must be escaped (`\"`) and cannot hit by
/// mistake.
fn has_substring(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Extract the value of `"timestamp":"…"` straight from the line bytes (no
/// full-line decode). Takes the **last** occurrence: an assistant line's
/// top-level timestamp sits near the end of the line, and escaped
/// timestamps inside bodies cannot hit by mistake (values longer than 40
/// bytes count as no timestamp; magpie tsAt(last) precedent).
fn timestamp_bytes(line: &[u8]) -> Option<i64> {
    let start = line
        .windows(TIMESTAMP_MARK.len())
        .rposition(|w| w == TIMESTAMP_MARK)?
        + TIMESTAMP_MARK.len();
    let rest = &line[start..];
    let end = rest.iter().position(|&b| b == b'"')?;
    if end == 0 || end > 40 {
        return None;
    }
    Some(parse_rfc3339_ms(std::str::from_utf8(&rest[..end]).ok()?))
}

fn parse_rfc3339_ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}
