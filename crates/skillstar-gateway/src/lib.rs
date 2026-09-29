//! Local model gateway. This crate owns protocol translation, the stream hold,
//! one smart ordering, the loopback listener, the Claude process bridge, and
//! the Codex config writer. It does not own provider keys, usage accounts, or
//! the decision model.

mod claude;
mod codex;
mod hold;
mod outbound;
mod route;
mod serve;
mod translate;

pub use claude::{
    AccountSnapshot, CallbackOutcome, ClaudeBridge, ClaudeError, ClaudeLaunch, ClaudeRun,
    ClaudeTool, IDLE_LONGEST, IDLE_MOST, PARK_LONGEST, STDERR_CAP, TEMP_PREFIX, TURN_ABORT,
    ToolResult, begin_callback, callback_token, find_claude_binary, listener_bridge,
    run_mcp_helper,
};
pub use codex::{ApplyError, CodexRoute, apply_agent, release_agent};
pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use outbound::{clear_outbound_log, outbound_log};
pub use route::{AllowanceSnapshot, RouteCandidate, USED_SHARE, route_smart};
pub use serve::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, REFUSED_PORT,
    ServeError, ServeOptions, Stop, resolve_addr, serve,
};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
