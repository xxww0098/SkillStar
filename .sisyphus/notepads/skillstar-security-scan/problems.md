# skillstar-security-scan: problems

## Unresolved: skillstar-ai public API surface

Need to verify whether `chat_completion` and `chat_completion_capped` are exported from `skillstar-ai`'s public API. If not, a compatibility shim in the app crate will be needed. **This is the critical path blocker for Option A.**

## Unresolved: Streaming event names in scan crate

The `ai://translate-stream` / `ai://summarize-stream` event names used by AI analysis are currently in the app command layer. Confirm whether the scan crate should own these or the app should.

## Unresolved: CLI command surface

`security_scan/mod.rs` appears to be the CLI scan entry point too (based on README: `skillstar scan`). The extraction needs to keep CLI behavior working without forcing Tauri IPC for every call.
