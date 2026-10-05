//! Codex rollout session parsing.
//!
//! A rollout is one JSONL file per session segment: a `session_meta` line, a
//! `turn_context` per turn naming the model (and effort) in force, and
//! `token_count` events carrying the session's **running totals** plus the
//! last turn's own usage (magpie codex.go evidence). All parsing rules are
//! pinned by tests (see codex_tests.rs).
//!
//! ## Token semantics (the family matrix, entry: input / output)
//!
//! - **codex**: `input_tokens` **includes** the cached tokens
//!   (`cached_input_tokens` is a subset of input), so cache_read is stripped
//!   from input before reporting ([`spent`]; magpie `spent` precedent).
//! - **claude / opencode / pi / omp**: input already **excludes** the cache
//!   portion; their usage fields map onto [`SessionTokens`] as they stand.
//!
//! ## Incremental model
//!
//! - Plain rollouts resume like the claude family (checkpoint head/prefix
//!   checks); the private state keeps the running total last seen so appended
//!   `token_count` events keep being counted by difference.
//! - `.jsonl.zst` rollouts (the Codex app's "compress local chat history")
//!   are written whole, so they are **always decoded and read whole** — no
//!   resume (magpie `packed` precedent). `from`/`to` of a packed rollout's
//!   calls are offsets into the **decompressed** text.
//! - `token_count` deltas: a total that grew on the previous one counts the
//!   difference; the same total told again adds nothing; a total that
//!   started over (or the first count of a file, whose total may run on from
//!   an earlier segment) counts `last_token_usage` instead
//!   (magpie cxCounted precedent).

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::checkpoint;
use super::codex_discovery::{ZST_SUFFIX, rollout_thread_id};
use super::{FileCheckpoint, SessionCall, SessionFile, SessionParser, SessionTokens};

/// Parser-private version: bump when codex parsing semantics change (old
/// checkpoints reread in full).
pub(crate) const CODEX_PARSER_VERSION: u32 = 1;

/// Byte markers deciding whether a line is worth decoding (rollouts write
/// `"type":"…"` near the line head; quotes inside string values would be
/// escaped and cannot hit by mistake).
const META_MARK: &[u8] = br#""type":"session_meta""#;
const TURN_MARK: &[u8] = br#""type":"turn_context""#;
const COUNT_MARK: &[u8] = br#""type":"token_count""#;
const SETTINGS_MARK: &[u8] = br#""type":"thread_settings_applied""#;
const STARTED_MARK: &[u8] = br#""type":"task_started""#;
const USER_MSG_MARK: &[u8] = br#""type":"user_message""#;
const USER_ROLE_MARK: &[u8] = br#""role":"user""#;
const TOOL_OUT_MARK: &[u8] = br#""type":"function_call_output""#;
const CUSTOM_OUT_MARK: &[u8] = br#""type":"custom_tool_call_output""#;

/// Session parser for the Codex CLI.
pub(crate) struct CodexParser;

impl SessionParser for CodexParser {
    const AGENT: &'static str = "codex";
    const PARSER_VERSION: u32 = CODEX_PARSER_VERSION;

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        super::codex_discovery::codex_files(Self::AGENT, home)
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        let packed = file
            .path
            .as_os_str()
            .to_str()
            .is_some_and(|p| p.ends_with(ZST_SUFFIX));
        let head = checkpoint::read_head(&file.path);
        // Packed rollouts are written whole, so they are read whole: no fast
        // path and no resume, whatever the checkpoint says.
        let (mut state, start_offset) = if packed {
            (CodexState::default(), 0)
        } else if let Some(prior) = &prior
            && prior.version == Self::PARSER_VERSION
            && prior.size == file.size
            && checkpoint::head_matches(&head, &prior.head_hash)
        {
            // Not grown and head fingerprint matches: hand the checkpoint
            // back unchanged without opening the body.
            return (Vec::new(), prior.clone());
        } else {
            match prior.as_ref() {
                Some(prior)
                    if checkpoint::can_resume(
                        prior,
                        Self::PARSER_VERSION,
                        file.size,
                        &head,
                        &checkpoint::prefix_hash(&file.path, prior.size),
                    ) =>
                {
                    (CodexState::from_checkpoint(prior), prior.offset)
                }
                _ => (CodexState::default(), 0),
            }
        };
        if state.session.is_empty()
            && let Some(thread) = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(rollout_thread_id)
        {
            // The thread id from the file name until session_meta names it.
            state.session = thread;
        }

        let mut delta = Vec::new();
        if packed {
            // Decode the whole frame first; a corrupt frame degrades to a
            // warn and an empty view (the next run retries).
            match decode_zst(&file.path) {
                Ok(bytes) => scan_lines(&bytes, 0, Self::AGENT, &mut state, &mut delta, &file.path),
                Err(error) => {
                    tracing::warn!(%error, path = ?file.path, "Failed to decode packed rollout; skipping this run")
                }
            }
        } else if let Ok(reader) = std::fs::File::open(&file.path).map(|f| {
            let mut f = f;
            let _ = f.seek(SeekFrom::Start(start_offset));
            BufReader::with_capacity(1 << 20, f)
        }) {
            let mut reader = reader;
            let mut buffer = Vec::new();
            let mut offset = start_offset;
            loop {
                buffer.clear();
                match reader.read_until(b'\n', &mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if !buffer.ends_with(b"\n") {
                            break; // A half-written line: not consumed
                        }
                        let line_end = offset + n as u64;
                        let trimmed: &[u8] = match buffer.last() {
                            Some(b'\r') | Some(b'\n') => &buffer[..buffer.len() - 1],
                            _ => &buffer[..],
                        };
                        codex_line(
                            Self::AGENT,
                            &mut state,
                            &mut delta,
                            trimmed,
                            &file.path,
                            line_end,
                        );
                        offset = line_end;
                    }
                }
            }
            state.consumed = offset;
        }

        let checkpoint = FileCheckpoint {
            version: Self::PARSER_VERSION,
            head_hash: checkpoint::head_hash(&head),
            prefix_hash: checkpoint::prefix_hash(&file.path, file.size),
            size: file.size,
            // Packed rollouts carry no meaningful byte offset (the frame is
            // decoded whole every run); the compressed size marks it read.
            offset: if packed { file.size } else { state.consumed },
            agent_state: state.to_json(),
            calls_seen: state.calls.len() as u64,
        };
        (delta, checkpoint)
    }

    fn unchanged(&self, file: &SessionFile, prior: &FileCheckpoint) -> bool {
        // Packed rollouts are rewritten whole; the plain fast path would
        // skip the decode and keep a stale view.
        if file
            .path
            .as_os_str()
            .to_str()
            .is_some_and(|path| path.ends_with(ZST_SUFFIX))
        {
            return false;
        }
        checkpoint::is_unchanged(prior, file, Self::PARSER_VERSION)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        // Codex calls carry no message id: they do not participate in
        // cross-file dedup (a resumed segment's totals start over, so the
        // delta semantics already keep segments from double counting).
        CodexState::from_checkpoint(checkpoint)
            .calls
            .into_iter()
            .map(|call| (String::new(), call))
            .collect()
    }
}

/// Scan whole lines of an already-decoded buffer (packed rollouts), starting
/// at byte `start` of the decompressed text.
fn scan_lines(
    bytes: &[u8],
    start: u64,
    agent: &'static str,
    state: &mut CodexState,
    delta: &mut Vec<SessionCall>,
    file: &Path,
) {
    let mut offset = start;
    let mut rest = bytes;
    while let Some(index) = rest.iter().position(|&b| b == b'\n') {
        let line = &rest[..index];
        let line_end = offset + index as u64 + 1;
        codex_line(agent, state, delta, line, file, line_end);
        rest = &rest[index + 1..];
        offset = line_end;
    }
    state.consumed = offset;
}

/// Decode a whole zstd frame (ruzstd, pure-Rust decode-only).
fn decode_zst(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(file).map_err(std::io::Error::other)?;
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Line shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CodexLine {
    #[serde(rename = "type", default)]
    line_type: String,
    #[serde(default)]
    payload: CodexPayload,
}

#[derive(Deserialize, Default)]
struct CodexPayload {
    #[serde(rename = "type", default)]
    payload_type: String,
    #[serde(default)]
    id: String,
    #[serde(rename = "session_id", default)]
    session_id: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    effort: Option<LenientStr>,
    #[serde(rename = "thread_settings", default)]
    settings: ThreadSettings,
    #[serde(default)]
    info: Option<CxInfo>,
}

#[derive(Deserialize, Default)]
struct ThreadSettings {
    #[serde(default)]
    model: String,
}

#[derive(Deserialize)]
struct CxInfo {
    #[serde(rename = "total_token_usage", default)]
    total: Option<CxUsage>,
    #[serde(rename = "last_token_usage", default)]
    last: Option<CxUsage>,
}

/// Codex's own usage tuple, input with the cache in it (see the module
/// matrix); `output_tokens` already contains the reasoning tokens.
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct CxUsage {
    #[serde(rename = "input_tokens", default)]
    input: u64,
    #[serde(rename = "cached_input_tokens", default)]
    cached: u64,
    #[serde(rename = "cache_write_input_tokens", default)]
    cache_write: u64,
    #[serde(rename = "output_tokens", default)]
    output: u64,
}

impl CxUsage {
    fn is_monotonic_after(self, prev: CxUsage) -> bool {
        self.input >= prev.input && self.output >= prev.output && self.cached >= prev.cached
    }

    fn minus(self, prev: CxUsage) -> CxUsage {
        CxUsage {
            input: self.input.saturating_sub(prev.input),
            cached: self.cached.saturating_sub(prev.cached),
            cache_write: self.cache_write.saturating_sub(prev.cache_write),
            output: self.output.saturating_sub(prev.output),
        }
    }

    fn tokens(self) -> SessionTokens {
        SessionTokens {
            input: self.input.saturating_sub(self.cached),
            output: self.output,
            cache_read: self.cached,
            cache_write: self.cache_write,
        }
    }
}

/// A string field where a non-string shape (number, object, null) reads as
/// empty instead of failing the whole line (magpie ccStr precedent; used for
/// `effort`, which real rollouts write as null when the turn asked for none).
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
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(LenientStr(String::new()))
            }
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

// ---------------------------------------------------------------------------
// Parser state
// ---------------------------------------------------------------------------

/// The codex family's checkpoint-private state
/// (`FileCheckpoint::agent_state`).
#[derive(Default)]
struct CodexState {
    meta: bool,
    session: String,
    model: String,
    effort: Option<String>,
    /// The running total last seen (input with the cache in it).
    total: Option<CxUsage>,
    /// The last thing the model was given (a prompt, a tool's output, a
    /// turn's start): the latency baseline.
    last_in_ms: i64,
    /// The last call's end time.
    last_call_ms: i64,
    /// End offset of the last call's line (default `from` for new calls).
    last_end: u64,
    consumed: u64,
    calls: Vec<SessionCall>,
}

impl CodexState {
    fn from_checkpoint(checkpoint: &FileCheckpoint) -> Self {
        #[derive(Deserialize, Default)]
        struct Stored {
            #[serde(default)]
            meta: bool,
            #[serde(default)]
            session: String,
            #[serde(default)]
            model: String,
            #[serde(default)]
            effort: Option<String>,
            #[serde(default)]
            total: Option<CxUsage>,
            #[serde(default)]
            last_in_ms: i64,
            #[serde(default)]
            last_call_ms: i64,
            #[serde(default)]
            last_end: u64,
            #[serde(default)]
            calls: Vec<SessionCall>,
        }
        let stored: Stored =
            serde_json::from_value(checkpoint.agent_state.clone()).unwrap_or_default();
        Self {
            meta: stored.meta,
            session: stored.session,
            model: stored.model,
            effort: stored.effort,
            total: stored.total,
            last_in_ms: stored.last_in_ms,
            last_call_ms: stored.last_call_ms,
            last_end: stored.last_end,
            // Recomputed by the scan for packed files; plain files keep it
            // from the offset the caller resumes from.
            consumed: checkpoint.offset,
            calls: stored.calls,
        }
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "meta": self.meta,
            "session": self.session,
            "model": self.model,
            "effort": self.effort,
            "total": self.total,
            "last_in_ms": self.last_in_ms,
            "last_call_ms": self.last_call_ms,
            "last_end": self.last_end,
            "calls": self.calls,
        })
    }
}

/// Handle one rollout line: sniff first, decode only the lines that can say
/// something about calls (the bulk of a rollout — compactions, tool output —
/// is stepped over without a JSON decode).
fn codex_line(
    agent: &'static str,
    state: &mut CodexState,
    delta: &mut Vec<SessionCall>,
    line: &[u8],
    file: &Path,
    line_end: u64,
) {
    // What the model was given: a prompt or a tool's output (the latency
    // baseline); a turn's start also counts as the ask (magpie precedent).
    if has_substring(line, USER_MSG_MARK)
        || has_substring(line, USER_ROLE_MARK)
        || has_substring(line, TOOL_OUT_MARK)
        || has_substring(line, CUSTOM_OUT_MARK)
        || has_substring(line, STARTED_MARK)
    {
        if let Some(at) = line_timestamp(line) {
            state.last_in_ms = at;
        }
        return;
    }
    let wanted = has_substring(line, COUNT_MARK)
        || has_substring(line, TURN_MARK)
        || has_substring(line, SETTINGS_MARK)
        || (!state.meta && has_substring(line, META_MARK));
    if !wanted {
        return;
    }

    let Ok(parsed) = serde_json::from_slice::<CodexLine>(line) else {
        return;
    };
    let payload = &parsed.payload;
    match (parsed.line_type.as_str(), payload.payload_type.as_str()) {
        ("session_meta", _) if !state.meta => {
            state.meta = true;
            let id = if payload.session_id.is_empty() {
                payload.id.as_str()
            } else {
                payload.session_id.as_str()
            };
            if !id.is_empty() {
                state.session = id.to_string();
            }
        }
        ("turn_context", _) => {
            if !payload.model.is_empty() {
                state.model = payload.model.clone();
            }
            // Every turn_context restates the effort: a turn that asked for
            // none clears the one before it (magpie precedent).
            state.effort = payload
                .effort
                .as_ref()
                .map(|e| e.0.clone())
                .filter(|e| !e.is_empty());
        }
        ("event_msg", "thread_settings_applied") => {
            if !payload.settings.model.is_empty() {
                state.model = payload.settings.model.clone();
            }
        }
        ("event_msg", "token_count") => {
            let Some(info) = &payload.info else { return };
            let Some(total) = info.total else { return };
            let delta_usage = match state.total {
                Some(prev) if total.is_monotonic_after(prev) => total.minus(prev),
                // The first count in the file (whose total may run on from an
                // earlier segment), or a total that started over.
                _ => info.last.unwrap_or(total),
            };
            state.total = Some(total);
            let tokens = delta_usage.tokens();
            if tokens.is_zero() {
                return; // The same total told again adds nothing
            }
            let Some(at) = line_timestamp(line) else {
                return;
            };
            // It took from what asked for it, or the call before it.
            let asked = state.last_in_ms.max(state.last_call_ms);
            let latency_ms = (asked > 0 && at > asked && at - asked < super::MAX_LATENCY_MS)
                .then_some((at - asked) as u64);
            state.calls.push(SessionCall {
                at,
                agent: agent.to_string(),
                session: state.session.clone(),
                model_asked: state.model.clone(),
                model_answered: state.model.clone(),
                tokens,
                effort: state.effort.clone(),
                request_id: None,
                error_kind: None,
                latency_ms,
                file: file.to_path_buf(),
                from: state.last_end,
                to: line_end,
            });
            delta.push(state.calls.last().cloned().expect("just pushed"));
            state.last_call_ms = at;
            state.last_end = line_end;
        }
        _ => {}
    }
}

/// The line's `"timestamp":"…"` value as epoch milliseconds (first occurrence,
/// scanned straight from the bytes; rollout lines open with it). Values longer
/// than 40 bytes count as no timestamp (magpie tsAt precedent).
fn line_timestamp(line: &[u8]) -> Option<i64> {
    const MARK: &[u8] = br#""timestamp":""#;
    let start = line.windows(MARK.len()).position(|w| w == MARK)? + MARK.len();
    let rest = &line[start..];
    let end = rest.iter().position(|&b| b == b'"')?;
    if end == 0 || end > 40 {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(std::str::from_utf8(&rest[..end]).ok()?)
        .ok()
        .map(|t| t.timestamp_millis())
}

/// Byte substring test (`slice::contains` only finds single elements).
fn has_substring(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
