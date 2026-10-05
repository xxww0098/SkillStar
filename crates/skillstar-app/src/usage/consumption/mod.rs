//! The session-file consumption view: one row per call the agents' own
//! session files know (slice 07, gateway merge removed with the model
//! domain, D-082).
//!
//! Every agent writes its own session records — Claude Code's
//! `projects/*.jsonl`, Codex's session files, OpenCode's, Pi's, OMP's —
//! and those files are now the only source: the gateway ledger that used
//! to be merged in here went away with the gateway, so the record↔call
//! correlation that deduplicated a through-proxied turn is gone too.
//! There is nothing left to double-count.
//!
//! The projection keeps the vocabulary the wire already speaks: ids keep
//! their original spelling, fields a session file cannot know stay
//! `None`, and the clock never enters here — [`consumption_view`] is a
//! pure function of the calls it is handed.

mod summarize;

pub use summarize::{
    Dimension, Group, Period, SeriesPoint, SummarizeInput, Summary, Totals, groups,
    period_floor_ms, summarize,
};

mod crossview;

pub use crossview::{today_consumption, today_from_rows};

use skillstar_usage::pricing::TokenCounts;
use skillstar_usage::sessions::SessionCall;

#[cfg(test)]
#[path = "consumption_tests.rs"]
mod tests;

/// One call as the consumption view lists it: the projection of a
/// session-file [`SessionCall`]. Fields the file cannot know arrive empty
/// or `None` rather than being invented; ids keep their original spelling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnifiedCall {
    /// Unix milliseconds: the call's own timestamp.
    pub at: i64,
    /// Owning agent id (the AGENT_SPECS vocabulary the session readers share).
    pub agent: String,
    /// The session id the agent named; empty when unknown.
    pub session: String,
    /// The model identity the agent asked for.
    pub model_asked: String,
    /// The model the reply named.
    pub model_answered: String,
    /// Token counts. The session vocabulary's four counts; `reasoning` is
    /// folded into `output` by the readers and reported as 0 here.
    pub tokens: TokenCounts,
    /// Effort hint, session files only.
    pub effort: Option<String>,
    /// Request id, session files only.
    pub request_id: Option<String>,
    /// Failure category (`None` = the call succeeded), the file's own word.
    pub error_kind: Option<String>,
    /// Turn duration as the file measured it.
    pub latency_ms: Option<u64>,
    /// Locate-and-reread triple: where the original conversation text lives.
    pub file: Option<std::path::PathBuf>,
    /// Byte offset the call's interval starts at (see `file`).
    pub from: Option<u64>,
    /// Byte offset the call's interval ends at (see `file`).
    pub to: Option<u64>,
}

impl UnifiedCall {
    /// Project a session-file call.
    fn from_session_call(call: &SessionCall) -> Self {
        Self {
            at: call.at,
            agent: call.agent.clone(),
            session: call.session.clone(),
            model_asked: call.model_asked.clone(),
            model_answered: call.model_answered.clone(),
            tokens: TokenCounts {
                input: call.tokens.input,
                output: call.tokens.output,
                cache_read: call.tokens.cache_read,
                cache_write: call.tokens.cache_write,
                reasoning: 0,
            },
            effort: call.effort.clone(),
            request_id: call
                .request_id
                .clone()
                .filter(|id| !id.is_empty()),
            error_kind: call
                .error_kind
                .clone()
                .filter(|kind| !kind.is_empty()),
            latency_ms: call.latency_ms,
            file: Some(call.file.clone()),
            from: Some(call.from),
            to: Some(call.to),
        }
    }
}

/// The consumption view over `calls`: every session call projected, newest
/// first (ties keep input order; the sort is stable). Pure — filtering by
/// period belongs to [`summarize`](summarize::summarize)'s UTC contract.
pub fn consumption_view(calls: &[SessionCall]) -> Vec<UnifiedCall> {
    let mut rows: Vec<UnifiedCall> = calls.iter().map(UnifiedCall::from_session_call).collect();
    rows.sort_by_key(|call| std::cmp::Reverse(call.at));
    rows
}
