# skillstar-ai Extraction Learnings

## Current Code Structure

### Boundary A: `commands/ai/` vs `core/ai_provider/`

**Commands layer** (`src-tauri/src/commands/ai/`):
- `translate.rs` — Tauri commands for SKILL.md translation, short text, batch
- `summarize.rs` — Tauri commands for skill summarization  
- `scan.rs` — Tauri commands for security scan pipeline
- `mod.rs` — Shared types (`AiStreamPayload`, config commands, re-exports)

Commands are **thin IPC wrappers**: validate input → call core → emit events.  
Zero business logic lives here — all strategy is in `core/ai_provider/`.

**Core layer** (`src-tauri/src/core/ai_provider/`):
- `config.rs` — `AiConfig`, `ApiFormat`, `FormatPreset`, serde defaults
- `http_client.rs` — HTTP client setup with reqwest
- `skill_pick.rs` — AI-assisted skill recommendation ranking
- `scan_params.rs` — Scan budget estimation (tokens, chunk sizes, concurrency)
- `constants.rs` — Magic numbers (`AI_MAX_TOKENS`, etc.)
- `mod.rs` — Load/save config with AES-256-GCM encryption, provider resolution,
  chat completion dispatch (OpenAI + Anthropic formats), markdown section splitting,
  streaming, translation validation

**Translation-specific sub-layer** (`src-tauri/src/core/ai/`):
- `translation_cache.rs` — SQLite translation cache (summary, short-text, skill-md)
- `translation_log.rs` — Per-scan translation audit logging
- `mdtx_bridge.rs` — Markdown-translator crate integration

### Boundary B: `core/ai/` vs `core/translation_api/`

`core/translation_api/` is already translation-API-specific (DeepL, DeepLX, MyMemory,
routing, markdown protection). It is NOT yet extracted as a crate but the directory
boundary is clear. The `ai_provider` module imports from `translation_api` via
`crate::core::translation_api::...` paths.

`core/ai/` is the "glue" layer that connects AI provider calls to translation caching
and logging. This will likely move into the future `skillstar-translation` crate
once extracted.

## Separation Strategy

### What belongs in `skillstar-ai` (extract now)

Pure AI/provider logic with **no app-specific wiring**:

1. **`config.rs`** — All `AiConfig` serde, defaults, `ApiFormat`, `FormatPreset`.  
   No Tauri types, no events.

2. **`http_client.rs`** — Reusable HTTP setup (reqwest client, header injection).  
   No app config, no disk paths.

3. **`constants.rs`** — Magic numbers scoped to AI provider budget calculations.

4. **`scan_params.rs`** — Token budget estimation from `context_window_k`.  
   Pure math, no side effects.

5. **`skill_pick.rs`** — Skill recommendation ranking. Input is skill catalog + user
   query; output is ranked list. No Tauri, no events, no SQLite.

6. **`mod.rs` logic** — Provider resolution, config load/save (AES encryption is
   app-specific path + machine-id, but the encryption scheme itself is reusable).
   Consider making path/machine-id injectable or configurable so the crate can be
   tested without a real machine.

### What stays in `commands/ai/` (thin compatibility shims)

After extraction, `commands/ai/` contains only:

- Tauri command handlers (1:1 mapping to public API surface)
- Event emission (`emit_*_stream_event`)
- Request/response type conversion (serde `Payload` ↔ core types)
- Input validation that must touch Tauri types (`tauri::Window`, `AppHandle`)

### What NOT to extract yet (belongs to `skillstar-translation` later)

The following are translation-specific and should remain in `core/ai/` or
`core/translation_api/` until T15 when `skillstar-translation` is extracted:

- `core/ai/translation_cache.rs` — Translation-specific cache schema/queries
- `core/ai/translation_log.rs` — Translation audit log schema/queries
- `core/ai/mdtx_bridge.rs` — Markdown-translator integration
- `core/ai_provider/mod.rs` — The `split_markdown_sections`, `translation_looks_translated`,
  prompt templates (`TRANSLATE_DOCUMENT_PROMPT`, etc.), chunk-based translation
  (`translate_text_in_chunks`, `translate_section_with_retry`)

### Keeping thin compatibility shims

After extracting `skillstar-ai` as a crate:

1. **In the crate**: `pub use config::{AiConfig, ApiFormat, FormatPreset};`  
   In `skillstar_lib`: `use skillstar_ai::{AiConfig, ApiFormat};`

2. **Path injection**: Config load needs data directory. Make paths configurable
   via a trait or a `DataPaths` struct passed to init. Avoid `machine_uid::get()`
   directly in the crate — inject it or make it mockable.

3. **No `tauri::` types in the crate public API** — keep `tauri::Window` and event
   names at the commands layer.

## Verifying Extraction with Unrelated Test Failures

The plan has `T12` (write TDD tests for Wave 2) AFTER `T11`. If broader test suite
has unrelated failures, use this hierarchy:

| Verification level | Scope | Run on |
|---|---|---|
| **Minimal** | `cargo check --package skillstar-ai` | Build/type only |
| **Unit** | `cargo test --package skillstar-ai` | Crate in isolation |
| **Integration** | `cargo test --package skillstar_lib -- --ignored` | Full app, filter to AI |
| **Smoke** | `cargo build --release` + startup check | Full app binary |

Do NOT run the full test suite if it has pre-existing failures. Instead:
- Use `cargo check --package skillstar-ai --all-features` as the baseline gate
- Document any pre-existing failures in issues.md without letting them block T11

## Patterns Observed from Existing Extractions

From reviewing `skillstar-model-config`, `skillstar-infra`, `skillstar-config`:

1. **Each crate is self-contained**: no `mod.rs` re-exports pulling from sibling crates
2. **Crate has its own `Cargo.toml`**: published path in workspace
3. **Crate types are `pub(crate)` or `pub`**: exposed publicly where needed
4. **App-specific logic stays in main crate**: disk paths, machine-id, Tauri events
5. **No `tauri` dependency in extracted crates**: keeps them pure and testable

## Recommended Extraction Order

1. Move `ai_provider/config.rs` + `constants.rs` + `scan_params.rs` first  
   (types + pure functions, no side effects)
2. Move `http_client.rs` second  
   (side-effectful but stateless — just client setup)
3. Move `skill_pick.rs` third  
   (pure business logic, no config persistence)
4. Extract the `mod.rs` provider resolution + chat dispatch last  
   (has config load/save which needs path injection design)
5. Keep `commands/ai/` as the thin Tauri wiring layer throughout

## Translation-Specific Logic to Avoid Extracting Too Early

- Prompt templates (`.md` files in `prompts/ai/`) — these belong to translation, not generic AI
- Markdown section splitting — translation-specific boundary logic
- `translation_looks_translated` — target-language-specific validation
- `translate_text_in_chunks` and all retry logic — translation orchestration
- The `translation_cache` module — SQLite schema tied to translation entries

These should all stay in `core/ai/` (which becomes `core/translation_api/` or the
future `skillstar-translation` crate) until T15.
