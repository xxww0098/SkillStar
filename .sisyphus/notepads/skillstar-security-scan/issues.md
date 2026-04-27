# skillstar-security-scan: issues

## Issue 1: Circular dependency risk with ai_provider

`security_scan/mod.rs` (line 20) imports `crate::core::ai_provider::{AiConfig, chat_completion, chat_completion_capped}`.

If we extract security_scan to `skillstar-scan` and make it depend on `skillstar-ai`, we need to verify that `skillstar-ai` exposes `chat_completion` and `chat_completion_capped` in its public API. Otherwise the app crate will need to re-export these through a compatibility shim.

**Status**: Needs verification — check `skillstar-ai` public exports.

## Issue 2: Infra paths dependency

`security_scan/mod.rs` uses `crate::core::infra::paths` for:
- `security_scan_log_path()`
- `security_scan_logs_dir()`
- `security_scan_policy_path()`

**Status**: The scan crate should define its own path logic using `dirs::data_dir()` + a hardcoded subdirectory. Do NOT depend on `skillstar-infra` to avoid circular deps.

## Issue 3: SQLite caching

The scan cache uses `rusqlite::Connection` but the connection is obtained from `skillstar-infra::db_pool`. 

**Status**: The scan crate should manage its own SQLite connection to `~/.skillstar/db/scan_cache.db` rather than sharing the infra pool. This keeps the scan crate independently testable.

## Issue 4: Over-extraction risk — command layer

The Tauri command handlers (in `commands/` or inline in mod.rs) should NOT be extracted. They are the app-specific wiring. The extraction should stop at `core/security_scan/` domain logic.

**Status**: Confirmed — leave commands in app crate.

## Issue 5: Streaming event names

Security scan uses `ai://translate-stream` and `ai://summarize-stream` event names for streaming. The scan crate should own these constants/event names as part of its public API.

**Status**: Noted — these are app-level Tauri event concerns, not domain concerns. Keep in command layer.
