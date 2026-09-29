//! Local model gateway. This crate owns protocol translation, the stream hold,
//! routing order, session affinity, the loopback listener, the Claude process bridge, and the
//! Codex config writer. It does not own provider keys, usage accounts, or the
//! decision model.

mod affinity;
mod claude;
mod codex;
mod codex_prompt;
mod hold;
mod outbound;
mod route;
mod serve;
mod surface;
mod translate;

pub use affinity::{
    AffinityChoice, AffinityMode, AffinityStick, AffinityTurn, AffinityWhy, CACHE_COLD,
    CACHE_WORTH, STICK_KEEP, affinity, keep_first, session_id,
};
pub use claude::{
    AccountSnapshot, CallbackOutcome, ClaudeBridge, ClaudeError, ClaudeLaunch, ClaudeRun,
    ClaudeTool, IDLE_LONGEST, IDLE_MOST, PARK_LONGEST, STDERR_CAP, TEMP_PREFIX, TURN_ABORT,
    ToolResult, begin_callback, callback_token, find_claude_binary, listener_bridge,
    run_mcp_helper,
};
pub use codex::{ApplyError, CodexRoute, apply_agent, release_agent};
pub use codex_prompt::{CODEX_COMPACT_PROMPT, CODEX_SUMMARY_PREFIX, COMPACTION_MARKER};
pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use outbound::{clear_outbound_log, outbound_log};
pub use route::{
    AllowanceSnapshot, RouteCandidate, RouteMode, RouteOwner, USED_SHARE, route_mode, route_smart,
    stored_route_mode,
};
pub use serve::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, REFUSED_PORT,
    ServeError, ServeOptions, Stop, resolve_addr, serve,
};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
