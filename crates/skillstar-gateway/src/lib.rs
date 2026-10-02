//! Local model gateway. This crate owns protocol translation, the stream hold,
//! routing order, session affinity, upstream rest, routing groups and their
//! rules, the intent classifier, secret redaction, vision transcription,
//! subscription signing, the listener, the Claude process bridge, the
//! Codex config writer, WSL Codex, the models.dev catalog cache, the
//! file-agent loopback writer, named profiles, model display names, effort
//! fitting, per-agent visible families, the install-level gateway key with the
//! LAN inbound gate, the persistent usage ledger, the effective model price,
//! and the in-memory ring of recent calls.
//! It does not own provider keys, usage accounts, or the decision model.

mod access;
mod agents;
mod catalog;
mod chatgpt;
mod claude;
mod codex;
mod codex_prompt;
mod cost;
mod effort;
mod ledger;
mod outbound;
mod redact;
mod route;
mod serve;
mod sign;
mod store;
mod surface;
mod trace;
mod translate;
mod visible;
mod vision;
mod wsl;

pub use access::{check_inbound, gateway_key};
pub use agents::{
    DESKTOP_PROFILE_ID, FILE_AGENTS, apply_gateway, cindy_imported, cindy_link, desktop_accepts,
    desktop_alias, desktop_dirs, desktop_effort_alias, token_for, written_loopback_label,
    written_model_ref,
};
pub use catalog::cache::{
    MODELS_DEV_URL, ModelsDevError, models_dev_cache_path, models_dev_load, models_dev_sync,
};
pub use catalog::ids::catalog_ids;
pub use claude::{
    AccountSnapshot, CallbackOutcome, ClaudeBridge, ClaudeError, ClaudeLaunch, ClaudeRun,
    ClaudeTool, IDLE_LONGEST, IDLE_MOST, PARK_LONGEST, STDERR_CAP, TEMP_PREFIX, TURN_ABORT,
    ToolResult, begin_callback, bridge_effort_arg, callback_token, find_claude_binary,
    listener_bridge, run_mcp_helper,
};
pub use codex::{ApplyError, CodexRoute, apply_agent, apply_agent_with_model, release_agent};
pub use codex_prompt::{CODEX_COMPACT_PROMPT, CODEX_SUMMARY_PREFIX, COMPACTION_MARKER};
pub use cost::{ModelCost, effective_price};
pub use effort::{apply_upstream_effort, model_efforts};
pub use ledger::{ErrorKind, Record, TokenCounts, append, key_fingerprint, load};
pub use outbound::{clear_outbound_log, outbound_log};
pub use redact::{mask_outbound, unmask_response};
pub use route::affinity::{
    AffinityChoice, AffinityMode, AffinityStick, AffinityTurn, AffinityWhy, CACHE_COLD,
    CACHE_WORTH, STICK_KEEP, affinity, keep_first, session_id,
};
pub use route::classify::{
    CLASSIFY_KEEP, CLASSIFY_REST, CLASSIFY_TIMEOUT, ClassifyCall, ClassifyReply, ClassifyTurn,
    JEV_SURE, ROUTER_USER_AGENT, order_with_classifier, stored_classifier,
};
pub use route::groups::{ServedModel, expand_group};
pub use route::hold::{HOLD_LONGEST, HOLD_MOST, HoldWriter};
pub use route::order::{
    AllowanceSnapshot, RouteCandidate, RouteMode, USED_SHARE, route_mode, route_smart,
};
pub use route::rest::{
    CREDIT_REST, FALLBACK_COOLDOWN, LONGEST_QUOTA, LONGEST_RETRY, LONGEST_WAIT, QUOTA_REST,
    RESETS_HEADER, Rest, RestSeat, UpstreamFailure, VERIFY_HOLD, VERIFY_REST, next_candidate,
    rest_after, verify_held,
};
pub use route::rules::{
    Caller, GroupRule, RuleRequest, order_with_rules, request_agent, stored_rules,
};
pub use serve::{
    ADDR_ENV, DEFAULT_ADDR, HEADER_READ_TIMEOUT, IDLE_TIMEOUT, PLACEHOLDER_BEARER, REFUSED_PORT,
    ServeError, ServeOptions, Stop, published_origin, resolve_addr, serve,
};
pub use sign::{AccountBook, ProviderSnapshot, SignInput, SignedUpstream, sign_upstream};
pub use store::groups::{
    GROUP_PREFIX, MAX_NEST, SaveGroupError, SavedGroup, save_group, stored_group_ids, stored_groups,
};
pub use store::listen::{SaveListenError, listen_label, save_listen};
pub use store::names::{SaveModelNameError, model_label, save_model_name, stored_model_names};
pub use store::profiles::{
    ApplyProfileError, ProfileAgent, ProfileApply, SaveProfileError, apply_profile, profile_names,
    save_profile,
};
pub use store::routing::{
    RouteOwner, SaveRoutingError, routing_state, save_routing, stored_route_mode,
};
pub use trace::{RecentCall, TRACE_KEEP, clear_recent_calls, note_recent_call, recent_calls};
pub use translate::{Protocol, TranslateError, outbound_body, upstream_body};
pub use visible::{catalog_serves, listed_ids, model_shown, shown_model_ids};
pub use vision::{
    VISION_CACHE, VISION_PARALLEL, VISION_SYSTEM, VISION_TIMEOUT, VISION_USER_AGENT, VisionCall,
    VisionReject, VisionReply, apply_vision,
};
pub use wsl::{
    WslCodex, apply_wsl_codex, wsl_codex_discover, wsl_codex_id, wsl_codex_list,
    wsl_codex_open_path,
};
