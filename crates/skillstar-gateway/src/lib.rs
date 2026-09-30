//! Local model gateway. This crate owns protocol translation, the stream hold,
//! routing order, session affinity, upstream rest, routing groups and their
//! rules, the intent classifier, secret redaction, vision transcription,
//! subscription signing, the loopback listener, the Claude process bridge, the
//! Codex config writer, WSL Codex, the models.dev catalog cache, and the
//! file-agent loopback writer.
//! It does not own provider keys, usage accounts, or the decision model.

mod affinity;
mod agents;
mod classify;
mod claude;
mod codex;
mod codex_prompt;
mod group;
mod hold;
mod models_dev;
mod outbound;
mod redact;
mod rest;
mod route;
mod rules;
mod serve;
mod sign;
mod surface;
mod translate;
mod vision;
mod wsl;

pub use agents::{
    DESKTOP_PROFILE_ID, FILE_AGENTS, apply_gateway, cindy_imported, cindy_link, desktop_accepts,
    desktop_alias, desktop_dirs, desktop_effort_alias, token_for, written_loopback_label,
};
pub use affinity::{
    AffinityChoice, AffinityMode, AffinityStick, AffinityTurn, AffinityWhy, CACHE_COLD,
    CACHE_WORTH, STICK_KEEP, affinity, keep_first, session_id,
};
pub use classify::{
    CLASSIFY_KEEP, CLASSIFY_REST, CLASSIFY_TIMEOUT, ClassifyCall, ClassifyReply, ClassifyTurn,
    JEV_SURE, ROUTER_USER_AGENT, order_with_classifier, stored_classifier,
};
pub use claude::{
    AccountSnapshot, CallbackOutcome, ClaudeBridge, ClaudeError, ClaudeLaunch, ClaudeRun,
    ClaudeTool, IDLE_LONGEST, IDLE_MOST, PARK_LONGEST, STDERR_CAP, TEMP_PREFIX, TURN_ABORT,
    ToolResult, begin_callback, callback_token, find_claude_binary, listener_bridge,
    run_mcp_helper,
};
pub use codex::{ApplyError, CodexRoute, apply_agent, release_agent};
pub use codex_prompt::{CODEX_COMPACT_PROMPT, CODEX_SUMMARY_PREFIX, COMPACTION_MARKER};
pub use group::{
    GROUP_PREFIX, MAX_NEST, SaveGroupError, ServedModel, expand_group, save_group, stored_group_ids,
};
pub use hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use models_dev::{
    MODELS_DEV_URL, ModelsDevError, models_dev_cache_path, models_dev_load, models_dev_sync,
};
pub use outbound::{clear_outbound_log, outbound_log};
pub use redact::{mask_outbound, unmask_response};
pub use rest::{
    CREDIT_REST, FALLBACK_COOLDOWN, LONGEST_QUOTA, LONGEST_RETRY, LONGEST_WAIT, QUOTA_REST,
    RESETS_HEADER, Rest, RestSeat, UpstreamFailure, VERIFY_HOLD, VERIFY_REST, next_candidate,
    rest_after, verify_held,
};
pub use route::{
    AllowanceSnapshot, RouteCandidate, RouteMode, RouteOwner, USED_SHARE, route_mode, route_smart,
    stored_route_mode,
};
pub use rules::{Caller, GroupRule, RuleRequest, order_with_rules, request_agent, stored_rules};
pub use sign::{AccountBook, ProviderSnapshot, SignInput, SignedUpstream, sign_upstream};
pub use serve::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, REFUSED_PORT,
    ServeError, ServeOptions, Stop, resolve_addr, serve,
};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
pub use vision::{
    VISION_CACHE, VISION_PARALLEL, VISION_SYSTEM, VISION_TIMEOUT, VISION_USER_AGENT, VisionCall,
    VisionReject, VisionReply, apply_vision,
};
pub use wsl::{
    WslCodex, apply_wsl_codex, wsl_codex_discover, wsl_codex_id, wsl_codex_list,
    wsl_codex_open_path,
};
