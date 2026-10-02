//! Read-only parsing of managed agents' local session files (the sessions foundation).
//!
//! This module only reads the agents' own session files (Claude Code's
//! `projects/*.jsonl` family first; codex/opencode/pi/omp follow in later
//! slices) and never writes into agent directories — the only path that
//! writes agent directories remains `apply_gateway`'s takeover mechanism
//! (the spec's global firewall rule 4). SkillStar's own derived incremental
//! index lives at `data_root()/sessions/index.json` (the [`checkpoint`]
//! module), atomically replaced via `atomic_write`.
//!
//! Incremental model (following the empirical lessons of magpie
//! internal/sessions):
//! - One [`FileCheckpoint`] per session file: `head_hash` (first 256 bytes)
//!   distinguishes "replaced vs grown", and `prefix_hash` is a sampled check
//!   of the already-read prefix; on any mismatch the file is reread from
//!   zero (truncation/replacement detection), otherwise reading resumes from
//!   `offset`, consuming only the newly added bytes.
//! - `SessionParser::parse` returns the delta calls added or overwritten in
//!   this run; [`read_calls`] rebuilds the full view via `replay` and then
//!   dedups by message id across files (resumed sessions copy old file
//!   content; the same msg id is counted once, earliest file first).
//!
//! All tests are sandboxed via `SKILLSTAR_TOOL_SYNC_HOME` /
//! `SKILLSTAR_DATA_DIR` and never touch the real `$HOME`.

mod checkpoint;
mod claude;
mod claude_discovery;
mod codex;
mod codex_discovery;
mod omp;
mod opencode;
mod opencode_discovery;
mod pi;

#[cfg(test)]
mod checkpoint_tests;
#[cfg(test)]
mod claude_tests;
#[cfg(test)]
mod codex_tests;
#[cfg(test)]
mod omp_tests;
#[cfg(test)]
mod opencode_tests;
#[cfg(test)]
mod pi_tests;

use claude::{ClaudeCodeParser, ClaudeDesktopParser};
use codex::CodexParser;
use omp::OmpParser;
use opencode::OpenCodeParser;
use pi::PiParser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use checkpoint::CheckpointStore;

/// The usage side's own four-field token tuple for one model call (no
/// dependency on gateway types).
///
/// `input` excludes the portion read from cache (cache_read is counted
/// separately).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl SessionTokens {
    /// All four fields zero (in the Claude family, a line whose usage is
    /// missing or all zero is not a call).
    pub fn is_zero(self) -> bool {
        self == Self::default()
    }
}

/// One model call recorded in an agent session file.
///
/// `file`/`from`/`to` are the locate-and-reread triple (magpie `Call`'s
/// File/From/To): the byte interval from the end of the previous assistant
/// line (or the start of its message's first block line) to the end of this
/// line; reread along it when the original conversation text is needed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionCall {
    /// Epoch milliseconds (consistent with the crate's `expires_at`
    /// millisecond precedent).
    pub at: i64,
    /// Owning agent id (`claude-code` / `claude-desktop` / ...).
    pub agent: String,
    /// The session id the agent sent to the gateway as the session header.
    pub session: String,
    /// The identity the agent was running as at request time (the Claude
    /// identity attachment's modelId); equals `model_answered` when the file
    /// carries no such information.
    pub model_asked: String,
    /// The answering model named in the file (for the Claude family, the
    /// model the vendor actually answered with).
    pub model_answered: String,
    pub tokens: SessionTokens,
    pub effort: Option<String>,
    pub request_id: Option<String>,
    /// Error category of an API error line (Claude family
    /// `isApiErrorMessage`); such a call carries no tokens.
    pub error_kind: Option<String>,
    pub latency_ms: Option<u64>,
    pub file: PathBuf,
    pub from: u64,
    pub to: u64,
}

/// The resume anchor for incremental parsing of one session file, persisted
/// at `data_root()/sessions/index.json` (keyed by file path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileCheckpoint {
    /// Parser version; when it differs from the current parser, this
    /// checkpoint is invalidated and the file is reread in full.
    pub version: u32,
    /// File head fingerprint (encoded as `v1:<length>:<hex>`, sha256 of the
    /// first 256 bytes). The length participates in the encoding because
    /// after a short file grows, the hash of its first 256 bytes naturally
    /// changes, so validation must truncate to the head length recorded in
    /// the checkpoint before comparing.
    pub head_hash: String,
    /// Sampled hash of the already-read prefix (`size` bytes)
    /// (`sample-v1:<hex>`).
    pub prefix_hash: String,
    /// File size when parsing last finished.
    pub size: u64,
    /// Resume offset: just past the last complete line.
    pub offset: u64,
    /// Parser-private resume state (claude family: msgs map / last
    /// timestamps / identity).
    pub agent_state: serde_json::Value,
    /// Count of (deduped) calls this file has seen in total; incremental and
    /// full rereads must converge to the same value.
    pub calls_seen: u64,
}

/// A discovered session file (the product of discovery).
///
/// `modified_ms` participates in cross-file ordering: resumed dedup is
/// "earliest file first".
#[derive(Debug, Clone, PartialEq)]
pub struct SessionFile {
    pub agent: &'static str,
    pub path: PathBuf,
    pub size: u64,
    pub modified_ms: i64,
}

/// A session parser for one agent family.
///
/// Implementations stay read-only: `parse` opens files for the sole purpose
/// of reading; all disk writes happen inside SkillStar's own data root
/// (performed by [`read_calls`] via [`checkpoint`], not inside the parser).
pub trait SessionParser: Send + Sync {
    /// The family's agent id (matching AGENT_SPECS: `claude-code` etc.).
    const AGENT: &'static str;
    /// Parser-private version number, written into
    /// [`FileCheckpoint::version`]; bump it when semantics change so old
    /// checkpoints reread in full.
    const PARSER_VERSION: u32;

    /// Find this family's session files under `home` (the sandbox root or
    /// the user home directory).
    fn discover(&self, home: &Path) -> Vec<SessionFile>;

    /// Incremental parsing: when `prior` is valid (version, head, prefix and
    /// size checks all pass), reading resumes from `prior`'s offset and the
    /// return value is the **delta calls added or overwritten in this run**;
    /// on check failure (truncation/replacement) or a missing `prior`, the
    /// file is reread from zero. When the file has not grown (`file.size ==
    /// prior.size`), return an empty delta and hand the checkpoint back
    /// unchanged.
    fn parse(&self, file: &SessionFile, prior: Option<FileCheckpoint>)
    -> (Vec<SessionCall>, FileCheckpoint);

    /// Rebuild this file's full call view from the checkpoint's private
    /// state. Returns `(message id, call)` pairs; an empty message id means
    /// the call does not participate in cross-file dedup.
    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)>;
}

/// Object-safe view of a parser's method face (`SessionParser` has
/// associated constants, so it is not dyn compatible). Any
/// `SessionParser + 'static` gets this view automatically; the registry
/// dispatches through it uniformly.
trait SessionParserMethods: Send + Sync {
    fn agent(&self) -> &'static str;
    fn discover(&self, home: &Path) -> Vec<SessionFile>;
    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint);
    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)>;
}

impl<P: SessionParser + 'static> SessionParserMethods for P {
    fn agent(&self) -> &'static str {
        Self::AGENT
    }

    fn discover(&self, home: &Path) -> Vec<SessionFile> {
        SessionParser::discover(self, home)
    }

    fn parse(
        &self,
        file: &SessionFile,
        prior: Option<FileCheckpoint>,
    ) -> (Vec<SessionCall>, FileCheckpoint) {
        SessionParser::parse(self, file, prior)
    }

    fn replay(&self, checkpoint: &FileCheckpoint) -> Vec<(String, SessionCall)> {
        SessionParser::replay(self, checkpoint)
    }
}

/// Parser registry. Slice 06 (codex/opencode/pi/omp) appends here.
fn parsers() -> &'static [Box<dyn SessionParserMethods>] {
    static PARSERS: std::sync::LazyLock<Vec<Box<dyn SessionParserMethods>>> =
        std::sync::LazyLock::new(|| {
            vec![
                Box::new(ClaudeCodeParser),
                Box::new(ClaudeDesktopParser),
                Box::new(CodexParser),
                Box::new(OpenCodeParser),
                Box::new(PiParser),
                Box::new(OmpParser),
            ]
        });
    &PARSERS
}

/// Read the session calls of all managed agents under `home`, returning the
/// **full view** (rebuilt on every call, so consumers can idempotently
/// replace the whole thing); `since` (epoch milliseconds) keeps only rows
/// with `at >= since`.
///
/// Flow (magpie callsFor precedent): incremental parse per file in ascending
/// file mtime order (earliest file first) → `replay` to the full view →
/// cross-file message-id dedup → since filter → reverse chronological order.
/// The checkpoint index is written back to `data_root()/sessions/index.json`;
/// a write failure only degrades to a `tracing::warn` (the next run rereads
/// in full) and never fails — this path does not write agent directories.
pub fn read_calls(home: &Path, since: Option<i64>) -> Vec<SessionCall> {
    let mut files = Vec::new();
    for parser in parsers() {
        files.extend(parser.discover(home));
    }
    // Earliest file first: resumed sessions copy old file content, and the
    // old file claims the message ids first.
    files.sort_by(|a, b| (a.modified_ms, &a.path).cmp(&(b.modified_ms, &b.path)));

    let store = CheckpointStore::load();
    // The new index keeps only files that still exist (checkpoints of
    // vanished files are pruned).
    let mut next = CheckpointStore::empty();
    let mut seen_msgs: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for file in &files {
        let Some(parser) = parsers().iter().find(|p| p.agent() == file.agent) else {
            continue;
        };
        let prior = store.get(&file.path).cloned();
        let (_delta, checkpoint) = parser.parse(file, prior);
        next.upsert(file.path.clone(), checkpoint.clone());
        for (msg, call) in parser.replay(&checkpoint) {
            if !msg.is_empty()
                && !seen_msgs.insert(msg.clone())
            {
                continue;
            }
            if since.is_none_or(|floor| call.at >= floor) {
                out.push(call);
            }
        }
    }
    if let Err(error) = next.save() {
        tracing::warn!(%error, index = ?next.path(), "Failed to save sessions checkpoint; next run will reread in full");
    }
    // Reverse chronological order; ties broken by file path and interval
    // end, descending (magpie callsFor ordering), so later calls within the
    // same file come first and the output is stable.
    out.sort_by(|a, b| (b.at, &b.file, b.to).cmp(&(a.at, &a.file, a.to)));
    out
}
