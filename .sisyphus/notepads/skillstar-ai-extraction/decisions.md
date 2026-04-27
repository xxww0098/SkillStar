# skillstar-ai Extraction Decisions

## Decision: What belongs in the extracted crate vs stays in app

**Extract (skillstar-ai crate)**:
- `AiConfig`, `ApiFormat`, `FormatPreset` types
- HTTP client setup (`reqwest`)
- Concurrency budget management (semaphore)
- Chat completion dispatch (OpenAI/Anthropic formats)
- Provider resolution from Models registry
- `skill_pick.rs` logic

**Stay in app / translation layer**:
- `translation_cache.rs`, `translation_log.rs`, `mdtx_bridge.rs`
- All markdown-splitting and chunk-retry logic
- Prompt templates (`.md` files under `prompts/ai/`)
- `translation_looks_translated`

## Decision: Compatibility shim pattern

After extraction, `src-tauri/src/core/ai_provider/` becomes a re-export shim:

```rust
// In skillstar_lib: src-tauri/src/core/ai_provider/mod.rs
pub use skillstar_ai::{
    AiConfig, ApiFormat, FormatPreset, load_config, save_config,
    chat_completion, chat_completion_capped, skill_pick::rank_skills,
    // ... re-export only what's needed by commands/ai/
};
```

The `commands/ai/` layer imports from `crate::core::ai_provider` unchanged,
so no refactoring needed downstream.

## Decision: Config path injection

Use trait-based injection to avoid direct `machine_uid::get()` calls in the crate:

```rust
pub trait MachineId {
    fn machine_id() -> String;
}
```

App provides `RealMachineId`, tests provide `MockMachineId`.
Alternatively, pass machine_id as a `DataPaths` struct at init time.
