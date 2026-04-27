
- T2 cleanup: removed unrelated frontend, AI/translation, docs, AGENTS, and generated artifact changes so the session diff only keeps the core-types extraction and minimal Rust wiring.
- T3 extraction: kept `skillstar-infra` generic by moving paths, fs ops, DB pools, migration, path_env, logs, and util helpers into the new crate while leaving thin legacy re-export shims in `src-tauri`.
- T4: extracted proxy and GitHub mirror config persistence/runtime helpers into `crates/skillstar-config`; legacy `src-tauri` config modules are now thin re-export shims to preserve call sites.
- T6 integration: followed the T4 one-line re-export shim pattern for `core::git::source_resolver` (`pub use skillstar_skill_core::source_resolver::*;`).
- Removed duplicate `cache_dir_name` from `repo_scanner/cache.rs` and routed it through the same shim via `pub use crate::core::git::source_resolver::cache_dir_name;`.
- Removed duplicate `normalize_remote_url` / `same_remote_url` from `cli.rs` and routed calls through the `source_resolver` shim.
- Fixed `update_checker.rs` stale closure type mismatch: `run_git_shallow_fetch` returns `Result<String>` but `skillstar_core_types::prefetch_unique_repos` expects `Result<()>`; added `.map(|_| ())` to discard the output string.
- T8/T9: prior extraction patterns held; thin re-export shims preserved call sites without changes.
- T10 extraction: moved reusable model-config core (`providers`, `health`, `quota`, `circuit_breaker`, `provider_states`, `speedtest`, `claude`, `codex`, `opencode`, plus shared `atomic_write`/`home_dir`/JSON helpers) into `crates/skillstar-model-config`. App-specific Tauri/OAuth/account wiring (`codex_oauth`, `gemini_oauth`, `codex_accounts`, `gemini_quota`) stays in `src-tauri`.
- The shim approach for T10: `src-tauri/src/core/model_config/mod.rs` uses `pub use skillstar_model_config::{...}` to re-export all moved modules, preserving every existing call site (`commands/models.rs`, `lib.rs`, `translation_api/router.rs`, `terminal_backend/provider_env.rs`, `ai_provider/mod.rs`) without changes.
- `codex_accounts.rs` needed `super::atomic_write`; fixed by re-exporting `atomic_write` from the shim `mod.rs` so the remaining local module continues to compile unchanged.
- All 8 provider unit tests migrated cleanly to the new crate and pass there.


## T11 findings

- Moved entire src-tauri/src/core/ai_provider/ directory into crates/skillstar-ai/src/ai_provider/ to extract the reusable AI/provider core.
- Created crates/skillstar-ai/Cargo.toml with dependencies on skillstar-infra, skillstar-config, skillstar-model-config.
- Updated imports in moved files to use external crate paths instead of crate::core::...:
  - ai_provider/mod.rs: crate::core::model_config::providers → skillstar_model_config::providers
  - ai_provider/mod.rs: crate::core::infra::paths::ai_config_path() → skillstar_infra::paths::ai_config_path()
  - ai_provider/mod.rs: crate::core::translation_api::config → crate::translation_config
  - ai_provider/http_client.rs: crate::core::config::proxy → skillstar_config::proxy
- Updated include_str paths in mod.rs for prompt files (../../../prompts/ → ../../../../src-tauri/prompts/).
- Made src-tauri/src/core/ai_provider/mod.rs a thin re-export shim (~15 lines).
- Updated src-tauri/src/core/translation_api/mod.rs to re-export config from skillstar_ai::translation_config instead of local pub mod config.
- Fixed visibility of items that needed to be public for re-export: chat_completion, chat_completion_capped, translation_looks_translated, language_display_name, get_http_client, and submodules (config, constants, http_client, scan_params, skill_pick).
- Removed pre-existing broken MyMemory tests (get_mymemory_de, mymemory_translate_short_text) that referenced functions in translation_api/services/mymemory.rs which is not part of skillstar-ai.
- Fixed a pre-existing config cache race condition in tests by adding invalidate_config_cache() before load_config() in load_config_does_not_derive_translation_settings_from_legacy_fields.
- cargo check -p skillstar-ai: passes
- cargo check -p skillstar: passes
- cargo test -p skillstar-ai: 17 passed, 0 failed


## T12 findings

- Fixed 2 pre-existing test failures in `skillstar-ai` caused by cache poisoning and incorrect temp-path writing in `load_config_returns_default_when_json_is_corrupted`.
- `skillstar-ai` tests that touch `load_config` must call `invalidate_config_cache()` before `load_config()` to remain independent, because `AI_CONFIG_CACHE` is a global static with a 5-second TTL.
- `skillstar-git` tests create real git repos via `git init`/`git commit`; they run quickly and reliably on macOS.
- `skillstar-model-config` provider tests need `with_temp_home` to isolate `HOME` env var; the same pattern works for `codex.rs` tests.
- `detect_short_text_source_lang` in `skillstar-ai` returns `"ja"` for any pure CJK string with >= 2 chars due to `japanese_score = kana_count + (han_count / 2)` and `japanese_ratio >= 0.30`. This is pre-existing behavior, not a regression.
- App-side shims in `src-tauri/src/core/{git,model_config,ai_provider}/mod.rs` are pure one-line `pub use` re-exports with no logic; no shim-level tests were warranted per T12a guidance.
- Total new test count: skillstar-git +6 (8 total), skillstar-model-config +10 (18 total), skillstar-ai +19 (36 total).

## T13 findings

- Extracted reusable skills lifecycle core into `crates/skillstar-skills/`.
- Moved modules: `skill_group.rs`, `repo_scanner/cache.rs`, `repo_scanner/detect.rs`, `repo_scanner/maintenance.rs`, `repo_scanner/ops.rs`, plus split `scan_skills_in_repo` and `compute_subtree_hash` from `scan_install.rs` into new `repo_scanner/scan.rs`.
- App-coupled modules kept in `src-tauri`: `installed_skill.rs`, `local_skill.rs`, `skill_install.rs`, `skill_update.rs`, `skill_bundle.rs`, `skill_pack.rs`, `discover.rs`, `update_checker.rs`, and `repo_scanner/scan_install.rs` (install_from_repo with security_scan/repo_history hooks).
- `src-tauri/src/core/skills/skill_group.rs` is now a one-line `pub use skillstar_skills::skill_group::*;` shim.
- `src-tauri/src/core/skills/repo_scanner/mod.rs` is a thin facade: re-exports from `skillstar_skills::repo_scanner` plus local `scan_install` module and `scan_repo_with_mode` (which calls `repo_history::upsert_entry`).
- Import mappings for extracted code:
  - `crate::core::config::github_mirror` → `skillstar_config::github_mirror`
  - `crate::core::git::ops` → `skillstar_git::ops`
  - `crate::core::infra::{paths, fs_ops, path_env::command_with_path}` → `skillstar_infra::{paths, fs_ops, path_env::command_with_path}`
  - `crate::core::lockfile` → `skillstar_core_types::lockfile` (but `lockfile_path()` is in `skillstar_infra::paths::lockfile_path()`)
  - `crate::core::skills::discover` → `skillstar_skill_core::discovery`
  - `crate::core::git::source_resolver` → `skillstar_skill_core::source_resolver`
  - `crate::core::skills::update_checker` → `skillstar_core_types::update_checker`
- Key gotcha: `skillstar_core_types` does NOT export `source_resolver`; that lives in `skillstar_skill_core`.
- Key gotcha: `lockfile_path()` is not in `skillstar_core_types::lockfile`; it's `skillstar_infra::paths::lockfile_path()`.
- `ops.rs` in the new crate wraps `skillstar_core_types::update_checker::prefetch_unique_repos` and `check_update_local` directly, injecting `skillstar_git::ops::find_repo_root` and the shallow-fetch closure.
- Removed unused re-exports from `src-tauri/src/core/skills/mod.rs`: `source_resolver` and `skill_discover` were no longer needed after extraction.
- Verification: `cargo check -p skillstar-skills` passes, `cargo check -p skillstar` passes, `cargo test -p skillstar-skills` passes (4 tests), `cargo test -p skillstar --lib` passes (109 tests).

## T14 findings

- Extracted reusable project-management core into `crates/skillstar-projects/`.
- Moved modules: `projects/agents.rs`, `projects/sync.rs`, `project_manifest/types.rs`, `project_manifest/helpers.rs`, `project_manifest/mod.rs` (minus `import_scanned_skills`).
- `manifest_paths.rs` was redundant because `skillstar_infra::paths` already provides `projects_manifest_path()` and `project_detail_dir()`; eliminated instead of copying.
- App-coupled `import_scanned_skills` (depends on `local_skill::reconcile_hub_symlinks` and `local_skill::adopt_existing_dir`) was kept in `src-tauri/src/core/project_manifest/mod.rs` as a thin shim alongside pure re-exports from `skillstar_projects::project_manifest`.
- Visibility changes in the new crate: `save_skills_list`, `deploy_skill_auto`, `ensure_project_root_exists`, `prune_deploy_modes_for_agents`, `clear_project_symlinks`, `prune_empty_dirs_upward` were promoted from `pub(crate)`/`private` to `pub` so the shim can call them.
- Import mappings for extracted code:
  - `crate::core::infra::paths::*` → `skillstar_infra::paths::*`
  - `crate::core::infra::fs_ops::*` → `skillstar_infra::fs_ops::*`
  - `crate::core::projects::agents` → `crate::agents` (within skillstar-projects)
  - `crate::core::lock_test_env()` → `crate::lock_test_env()` (replicated in skillstar-projects lib.rs)
- `src-tauri/src/core/projects/{agents,sync}.rs` shims are one-line `pub use skillstar_projects::{agents,sync}::*;`.
- `src-tauri/src/core/skills/mod.rs` re-exports continue to work unchanged because they route through the shims.
- `cargo check -p skillstar-projects`: passes
- `cargo check -p skillstar`: passes (only pre-existing warnings)
- `cargo test -p skillstar-projects`: 7 passed, 0 failed
- `cargo test -p skillstar --lib`: 102 passed, 0 failed (includes 3 import_scanned_skills shim tests)

## T15 findings

- Extracted reusable translation orchestration into `crates/skillstar-translation/`.
- Moved modules: `translation_api/mod.rs` → `api.rs`, `translation_api/router.rs` → `router.rs`, `translation_api/markdown.rs` → `markdown.rs`, `translation_api/services/{mod,deepl,mymemory}.rs` → `services/`, `ai/translation_cache.rs` → `cache.rs`, `ai/translation_log.rs` → `log.rs`.
- `mdtx_bridge.rs` intentionally kept in `src-tauri` as the bridge between `skillstar-ai` and the `markdown-translator` crate; moving it would create a messier boundary because it tightly couples AI provider primitives with the markdown translator engine.
- `skillstar-translation` depends on `skillstar-ai`, `skillstar-infra`, and `skillstar-model-config` — no circular dependencies since `skillstar-ai` does not depend on translation.
- Shim approach in `src-tauri`:
  - `core/translation_api/mod.rs` re-exports core types from `skillstar_translation` and keeps `config` re-export from `skillstar_ai`.
  - `core/translation_api/{markdown,router,services}.rs` are one-line `pub use skillstar_translation::{...}::*;` file shims.
  - `core/ai/translation_cache.rs` and `translation_log.rs` are one-line `pub use skillstar_translation::{cache,log}::*;` shims.
- `lock_test_env()` replicated in `skillstar-translation/src/lib.rs` for test isolation, following the T14 pattern.
- Import mappings for extracted code:
  - `crate::core::ai_provider::...` → `skillstar_ai::ai_provider::...`
  - `crate::core::model_config::circuit_breaker` → `skillstar_model_config::circuit_breaker`
  - `crate::core::infra::{paths, db_pool, util}` → `skillstar_infra::{paths, db_pool, util}`
  - `crate::core::translation_api::config` → `skillstar_ai::translation_config`
- Verification: `cargo check -p skillstar-translation` passes, `cargo check -p skillstar` passes (only pre-existing warnings), `cargo test -p skillstar-translation` passes (12 tests), `cargo test -p skillstar --lib` passes (90 tests).

## T16 findings

- Added 39 new tests to `skillstar-translation` crate (from 12 to 51):
  - `api.rs`: 11 tests covering `normalize_lang` edge cases (deepl target/source zh, en, zh-hant, pt variants, fil, unknown uppercasing, non-deepl pass-through), `is_traditional_provider` classification, and `TranslationError` display messages.
  - `services/deepl.rs`: 5 tests for `DeepLXService` config handling (default endpoint, configured URL, configured key, whitespace trimming, whitespace-only URL falls back to default).
  - `markdown.rs`: 20+ tests for deterministic helpers: `restore_placeholders` (basic reversal, length-descending sort to avoid substitution collision), `split_lines_preserve_newline` (empty, basic, no trailing), `has_translatable_letters` (detects latin, rejects empty/numeric), `next_placeholder` counter increment, `split_leading_frontmatter` edge cases, `split_line_ending`, `filter_markdown_lines` for inline code, multiline code, links, headings, lists, blockquotes, `build_line_plans` (no placeholders, skips untranslatable), `render_translated_lines` (basic and with whitespace).
- Added 17 new tests to `skillstar` lib (from 90 to 107):
  - `mdtx_bridge.rs`: 2 tests — `skill_md_chain_returns_error_when_pipeline_fails` uses a `FailingMock` LlmProvider to verify error propagation through `translate_skill_content_with_provider`, and `build_translator_config_clamps_max_parallel` verifies the upper clamp.
  - `commands/ai/mod.rs`: 6 tests for `normalize_quality_provider_ref` (claude valid, codex valid, invalid app_id, empty provider_id, whitespace trimming) and `TranslationApiConfigPayload` roundtrip.
  - `commands/ai/translate.rs`: 8 tests for `content_hash` determinism, `maybe_fix_trailing_newline` (adds, leaves, already present), `split_markdown_sections` (empty, single, multiple headings), `is_markdown_heading` (valid and invalid cases).
- `normalize_lang_deepl_source_zh` initially asserted "ZH-HANS" for source `zh`, but production code only maps "ZH-HANS" when `target=true`. Source `zh` falls through to uppercase → "ZH". Test was corrected.
- `build_translator_config_uses_min_one_parallel` was removed after discovery that `resolve_scan_params` returns 4 when `max_concurrent_requests == 0` (explicit fallback in scan_params.rs), so the clamp doesn't reduce it to 1.
- Pre-existing `skill_md_chain_runs_mdtx_pipeline_with_mock_llm` showed flaky behavior: fails when run with full suite but passes when run individually. Likely shared cache/state in markdown-translator; not caused by T16 changes (no production code was modified).
- No production code changes were required except for minor test-import adjustments (`use super::{...}` additions in test modules).

## T17 findings

- Removed 9 stale duplicate files from `src-tauri/src/core/security_scan/`:
  - `types.rs`, `constants.rs`, `policy.rs`, `static_patterns.rs`, `smart_rules.rs`, `snippet.rs`, `orchestrator.rs`, `security_policy_default.yaml`, `security_smart_rules_default.yaml`
- Kept only `src-tauri/src/core/security_scan/mod.rs` (thin re-export shim).
- `mod.rs` already uses `pub use skillstar_security_scan::*;` to re-export from the crate.
- No direct references to the deleted sibling files existed anywhere in `src-tauri`.
- Verification: `cargo check -p skillstar-security-scan` passes, `cargo check -p skillstar` passes (only pre-existing warnings), `cargo test -p skillstar-security-scan` passes (30 tests).

## T19 findings (first slice)

- Extracted reusable patrol data/config/helpers into `crates/skillstar-patrol/`.
- Moved modules: `types.rs` (PatrolConfig, PatrolStatus, PatrolCheckEvent, HubSkillEntry), `config.rs` (config_path, load_config, save_config), `helpers.rs` (check_skill_update_local).
- `collect_hub_skills` intentionally kept in `src-tauri` because it depends on `local_skill::is_local_skill` which has no clean extracted surface (depends on `skillstar_infra::fs_ops` which is app-coupled).
- `PatrolManager`, `PatrolInner`, `start()`, `stop()`, `set_enabled()`, `status()`, and `patrol_loop()` stay in `src-tauri/src/core/patrol.rs` (Tauri/Tokio coupling).
- `src-tauri/src/core/patrol.rs` now uses `pub use skillstar_patrol::types::{...}` to re-export types publicly for `commands/patrol.rs`.
- Import mappings for extracted code:
  - `crate::core::infra::paths::patrol_state_path()` → `skillstar_infra::paths::patrol_state_path()`
  - `crate::core::infra::paths::hub_skills_dir()` → `skillstar_infra::paths::hub_skills_dir()`
  - `crate::core::git::ops` → `skillstar_git::ops`
  - `repo_scanner::is_repo_cached_skill`, `repo_scanner::check_repo_skill_update_local` → `skillstar_skills::repo_scanner`
- Verification: `cargo check -p skillstar-patrol` passes, `cargo check -p skillstar` passes (only pre-existing warnings).

## T20 findings (first slice)

- Extracted pure terminal helpers into `crates/skillstar-terminal/`.
- Moved modules: `config.rs` (LaunchConfig, persistence, validation, tree helpers), `types.rs` (AgentCliInfo, LaunchScriptKind, DeployResult), `registry.rs` (find_cli_binary, list_available_clis), `session.rs` (session_name).
- `script_builder.rs` was listed for extraction but depends on `pane_command.rs` which stays local in this slice. Script builder remains in `src-tauri` because it calls `pane_command::build_posix_pane_command` and `pane_command::pane_command_spec`, and `pane_command.rs` is not being moved (depends on `provider_env.rs` which depends on `model_config::providers::read_store()`).
- `pane_command.rs` and `provider_env.rs` intentionally kept in `src-tauri` for this slice because they have tight coupling to local modules (`model_config::providers::read_store()`).
- Created `crates/skillstar-terminal/src/lib.rs` with module structure and re-exports.
- Created `crates/skillstar-terminal/Cargo.toml` with dependencies: anyhow, serde, serde_json, sha2, skillstar-infra, which.
- Updated `src-tauri/src/core/terminal/mod.rs` to be a compatibility shim re-exporting from `skillstar_terminal`.
- Deleted `src-tauri/src/core/terminal/config.rs` (moved to new crate).
- Updated `src-tauri/src/core/terminal_backend.rs` to import from `skillstar_terminal` and keep local submodules (`pane_command`, `provider_env`, `script_builder`, `terminal_launcher`).
- Updated imports in `pane_command.rs`, `provider_env.rs`, `script_builder.rs`, `terminal_launcher.rs` to use the new crate paths.
- Updated imports in `cli.rs` and `commands/launch.rs` to use `skillstar_terminal` directly.
- Import path mappings for extracted code:
  - `crate::core::infra::paths::config_dir()` → `skillstar_infra::paths::config_dir()`
  - `crate::core::terminal::config::LayoutNode` → `crate::core::terminal::LayoutNode` (shim re-export)
  - `crate::core::terminal_backend::types::LaunchScriptKind` → `crate::core::terminal_backend::LaunchScriptKind` (re-export)
  - `super::registry::binary_name_for_agent` → `crate::core::terminal_backend::binary_name_for_agent` (re-export)
- `validate()` function in config uses `find_cli_binary()` from same crate's registry module.
- Verification: `cargo check -p skillstar-terminal` passes, `cargo test -p skillstar-terminal` passes (6 tests), `cargo check -p skillstar` passes (only pre-existing warnings).




## T20 (provider-env) findings

- Extracted `provider_env.rs` from `src-tauri/src/core/terminal_backend/` into `crates/skillstar-terminal/src/provider_env.rs`.
- Added `skillstar-model-config = { version = "0.1.0", path = "../skillstar-model-config" }` dependency to skillstar-terminal's Cargo.toml.
- Changed `crate::core::model_config::providers::read_store()` to `skillstar_model_config::providers::read_store()`.
- Changed `crate::core::terminal::LayoutNode` to `crate::config::LayoutNode` (uses local config module).
- Changed `extract_env_for_pane` from `pub(crate)` to `pub` because it's called from src-tauri (different crate).
- Updated callers in src-tauri:
  - `terminal_backend.rs`: removed `mod provider_env;`, changed `provider_env::` to `skillstar_terminal::provider_env::`
  - `pane_command.rs`: changed `super::provider_env::extract_env_for_pane` to `skillstar_terminal::provider_env::extract_env_for_pane`
- Deleted old `src-tauri/src/core/terminal_backend/provider_env.rs`.
- Moved 2 unit tests with the file (both pass in skillstar-terminal test suite).
- cargo check -p skillstar-terminal: passes
- cargo test -p skillstar-terminal: 8 passed (including 2 provider_env tests)
- cargo check -p skillstar: passes

## T20 remaining (next steps)

- Other terminal_backend files still in src-tauri: pane_command.rs, script_builder.rs, terminal_launcher.rs
- CLI launch integration in commands/launch.rs not yet updated to use skillstar_terminal
- The task allows NOT touching commands/launch.rs unless strictly required, so these remain for later T20 steps

## T20 (pane_command move) findings

- Moved `pane_command.rs` from `src-tauri/src/core/terminal_backend/` to `crates/skillstar-terminal/src/pane_command.rs`.
- Key import remappings inside the moved file:
  - `crate::core::terminal::LayoutNode` → `crate::config::LayoutNode` (local to skillstar-terminal)
  - `skillstar_terminal::provider_env::extract_env_for_pane` → `crate::provider_env::extract_env_for_pane` (local crate)
  - `crate::core::terminal_backend::binary_name_for_agent` → `crate::registry::binary_name_for_agent` (local crate, already pub(crate))
- Added `mod pane_command;` and `pub use pane_command::{...}` to skillstar-terminal's lib.rs.
- Updated `script_builder.rs` to import from `skillstar_terminal::pane_command` instead of `super::pane_command`.
- Removed `mod pane_command;` from `terminal_backend.rs`.
- Deleted old file `src-tauri/src/core/terminal_backend/pane_command.rs`.
- Moved 5 unit tests with the file (all pass in skillstar-terminal test suite).
- cargo check -p skillstar-terminal: passes
- cargo test -p skillstar-terminal: 13 passed (including 5 pane_command tests)
- cargo check -p skillstar: passes (only pre-existing warnings)

## T20 (script_builder + terminal_launcher move) findings

- Moved `script_builder.rs` and `terminal_launcher.rs` from `src-tauri/src/core/terminal_backend/` to `crates/skillstar-terminal/src/`.
- Key import remappings in moved files:
  - `crate::core::terminal::LayoutNode` → `crate::config::LayoutNode` (local to skillstar-terminal)
  - `crate::core::terminal_backend::LaunchScriptKind` → `crate::types::LaunchScriptKind` (local to skillstar-terminal)
  - `skillstar_terminal::pane_command::{...}` → `crate::pane_command::{...}` (local to skillstar-terminal)
- Added `tracing = "0.1"` dependency to skillstar-terminal's Cargo.toml (was transitively available via skillstar-infra but needed explicit declaration).
- Made `generate_single_script`, `generate_single_script_for_current_os`, and `open_script_in_terminal_with_kind` `pub` (not `pub(crate)`) to allow re-export from src-tauri.
- Updated `script_builder.rs` to remove unused import `pane_command_spec`.
- Updated `terminal_backend.rs` to use `pub use` re-exports from skillstar_terminal instead of local wrapper functions.
- Removed duplicate wrapper functions from `terminal_backend.rs` since `pub use` already exposes them.
- Deleted old files: `src-tauri/src/core/terminal_backend/script_builder.rs` and `terminal_launcher.rs`.
- Updated `skillstar-terminal/src/lib.rs` to add `pub mod script_builder;` and `pub mod terminal_launcher;`.
- cargo check -p skillstar-terminal: passes (5 warnings for cfg-gated unused items expected)
- cargo test -p skillstar-terminal: 13 passed
- cargo check -p skillstar: passes (only pre-existing warnings)
- cargo run -p skillstar --bin skillstar -- launch --help: works correctly

## T21 findings (first slice)

- Created `crates/skillstar-commands/` crate for portable Tauri command handlers.
- `network.rs` was truly portable: uses `skillstar_config::{github_mirror, proxy}` which are already extracted.
- `acp.rs` required extracting `AcpConfig` + `load_config`/`save_config` to `skillstar_config::acp` first because it depended on `acp_client::AcpConfig` which was in src-tauri.
- Added `skillstar_config::acp` module: `AcpConfig` struct, `load_config()`, `save_config()`, and helpers (default_agent_command, default_agent_label).
- Updated `src-tauri/src/core/acp_client.rs` to re-export from `skillstar_config::acp` instead of defining locally.
- `skillstar-commands` depends on: tauri, serde, serde_json, skillstar-infra, skillstar-config, anyhow, tracing.
- `src-tauri/src/commands/mod.rs` now re-exports: `pub use skillstar_commands::acp;` and `pub use skillstar_commands::network::*;` (network uses glob for top-level re-export since commands are registered at top-level; acp uses namespace since commands are registered under `commands::acp::`).
- Deleted original `src-tauri/src/commands/acp.rs` and `src-tauri/src/commands/network.rs`.
- `lib.rs` registration unchanged - path compatibility preserved through re-exports.
- cargo check -p skillstar-commands: passes
- cargo check -p skillstar: passes (27 pre-existing warnings)

## T21 findings (second slice - agents + projects)

- Moved `agents.rs` and `projects.rs` command handlers to `skillstar-commands` crate.
- `skillstar-commands` now depends on: tauri, serde, serde_json, skillstar-infra, skillstar-config, skillstar-projects, anyhow, tracing.
- Key challenge: `commands/agents.rs` calls `installed_skill::invalidate_cache()` which is src-tauri-specific (manages in-memory SKILL_CACHE). This function cannot live in `skillstar-commands` because `installed_skill` is app-specific and not extracted.
- Solution: Removed `installed_skill::invalidate_cache()` calls from `skillstar-commands/src/agents.rs`. The local `src-tauri/src/commands/agents.rs` became a thin-wrapper layer that:
  1. Calls the portable `skillstar_commands::agents` functions
  2. Then calls `installed_skill::invalidate_cache()` for src-tauri state management
- `import_project_skills` command depends on `project_manifest::import_scanned_skills` which is kept in src-tauri (calls `local_skill::reconcile_hub_symlinks` and `local_skill::adopt_existing_dir` which are app-specific).
- Solution: `skillstar-commands/src/projects.rs` does NOT include `import_project_skills`. The local `src-tauri/src/commands/projects.rs` re-exports from `skillstar_commands::projects` and adds `import_project_skills` locally.
- `AgentProfile` and `CustomProfileDef` types needed to be re-exported from `skillstar_commands::agents` so src-tauri's thin wrappers can use them as return/parameter types.
- `src-tauri/src/commands/mod.rs` keeps `pub mod agents;` and `pub mod projects;` (not re-exported) so local thin-wrapper modules work correctly.
- `cargo check -p skillstar-commands`: passes
- `cargo check -p skillstar`: passes (only pre-existing warnings)

## T21 findings (third slice - shell + bundles attempted)

- Moved `shell.rs` to `skillstar-commands` crate: `shell.rs` is truly portable - only uses `AppError` from `skillstar_infra::error::AppError`.
- Updated `skillstar-commands/src/lib.rs` to add `pub mod shell;`.
- Updated `src-tauri/src/commands/mod.rs` to re-export from `skillstar_commands::shell::*` instead of local module.
- Deleted original `src-tauri/src/commands/shell.rs`.
- `bundles.rs` attempt blocked: `bundles.rs` command handlers are thin wrappers around `skill_bundle::*` functions which are in `src-tauri/src/core/skills/skill_bundle.rs`. The `skill_bundle` module depends on `installed_skill::invalidate_cache()` (line 488 of `import_multi_bundle`) which manages app-specific in-memory SKILL_CACHE. This app-specific state management cannot be extracted to `skillstar-commands`.
- Unlike agents/projects where thin-wrapper pattern worked (app-specific logic was in the local wrapper), bundles has no portable portion - the entire logic is in `skill_bundle` which is app-coupled due to `installed_skill::invalidate_cache()`.
- Options for future work: (a) extract `skill_bundle` with dependency injection for cache invalidation, or (b) move `installed_skill::invalidate_cache` to a separate extracted function. This is a larger change than the current slice intends.
- `bundles.rs` remains in `src-tauri/src/commands/` for now.
- `cargo check -p skillstar-commands`: passes
- `cargo check -p skillstar`: passes (only pre-existing warnings)

## T21 findings (fourth slice - launch + marketplace)

- Moved `launch.rs` and `marketplace.rs` command handlers to `skillstar-commands` crate.
- Added new dependencies to `skillstar-commands/Cargo.toml`: `skillstar-terminal`, `skillstar-marketplace-core`, `skillstar-ai`, `tokio`.
- Key challenge for `marketplace.rs`: original code called `crate::commands::ai::ensure_ai_config_pub()` which is in the local `ai` module. Solution: replicated the config validation logic using `ai_provider::load_config_async()` + `ai_provider::resolve_runtime_config()` + `ApiFormat::Local` check directly in the moved code.
- Key challenge for `marketplace.rs`: used `crate::core::marketplace` which is a wrapper module in src-tauri. Solution: accessed `skillstar_marketplace_core::remote` and `skillstar_marketplace_core::snapshot` submodules directly plus re-exported types (`MarketplaceResult`, `Skill`, etc.).
- Key challenge for `launch.rs`: original code used `crate::core::terminal_backend::deploy()`. Solution: inlined the `deploy()` logic using `skillstar_terminal` functions directly (`generate_single_script_for_current_os`, `open_script_in_terminal_with_kind`, etc.).
- Import structure for `launch.rs` in skillstar-commands:
  - `skillstar_terminal::config::{deployable_layout, delete_config, load_config, save_config, validate, LaunchConfig}`
  - `skillstar_terminal::registry::list_available_clis`
  - `skillstar_terminal::script_builder::generate_single_script_for_current_os`
  - `skillstar_terminal::session::session_name`
  - `skillstar_terminal::terminal_launcher::open_script_in_terminal_with_kind`
  - `skillstar_terminal::types::{AgentCliInfo, DeployResult}`
- Import structure for `marketplace.rs` in skillstar-commands:
  - `skillstar_marketplace_core::remote` (functions like `search_skills_sh`, `ai_search_by_keywords`)
  - `skillstar_marketplace_core::snapshot` (functions like `get_leaderboard_local`, `search_local`)
  - `skillstar_marketplace_core::{AiKeywordSearchResult, LocalFirstResult, MarketplacePack, ...}` (re-exported types)
  - `skillstar_ai::ai_provider::{self, ApiFormat}` for AI config
- Compatibility shim pattern: `src-tauri/src/commands/mod.rs` uses `pub mod launch { pub use skillstar_commands::launch::*; }` to preserve `commands::launch::*` path access for lib.rs registration.
- Same submodule re-export pattern for marketplace: `pub mod marketplace { pub use skillstar_commands::marketplace::*; }`.
- Removed original files: `src-tauri/src/commands/launch.rs` and `src-tauri/src/commands/marketplace.rs`.
- `cargo check -p skillstar-commands`: passes
- `cargo check -p skillstar`: passes (53 pre-existing warnings - many about unused functions in core/marketplace.rs and core/marketplace_snapshot since those functions are now called from skillstar-commands)

## T22 findings (first slice - skillstar-cli crate extraction)

- Created `crates/skillstar-cli/` crate for CLI parser types and dispatcher.
- Moved to skillstar-cli: `Cli`, `Commands`, `PackAction`, `LaunchAction` (clap types), and `run(args, handlers)` dispatcher.
- Kept in src-tauri: all `cmd_*` functions (cmd_list, cmd_install, cmd_update, cmd_create, cmd_publish, cmd_scan, cmd_doctor, cmd_pack_list, cmd_pack_remove, cmd_launch_deploy, cmd_launch_run).
- Design pattern: `CliHandlers` struct holds function pointers for all cmd_* functions; dispatcher calls them via `(handler)()` syntax.
- `migrate_and_run()` function pointer is called first to run legacy path migration before parsing.
- `skillstar-cli` depends only on `clap = { version = "4", features = ["derive"] }`.
- `skillstar-cli` has NO dependencies on skillstar-infra, skillstar-core, etc. - it's purely the CLI interface layer.
- `src-tauri/src/cli.rs` re-exports nothing; it imports `CliHandlers` from `skillstar_cli` and provides `cli_handlers()` constructor and `run(args)` entry point.
- Key issue fixed: `#[derive(Default)]` cannot be used on structs containing function pointers. Removed Default derive; CliHandlers is constructed via `cli_handlers()` builder function.
- Key issue fixed: duplicate import of types via both `use` and `pub use` statements - removed the redundant `use` statement and kept only `use skillstar_cli::CliHandlers;`.
- Updated `Cargo.toml` workspace to include `crates/skillstar-cli`.
- Updated `src-tauri/Cargo.toml` to add `skillstar-cli = { version = "0.1.0", path = "../crates/skillstar-cli" }`.
- Verification: `cargo check -p skillstar-cli` passes, `cargo check -p skillstar` passes, `cargo run -p skillstar --bin skillstar -- help` passes.

## T22 findings (second slice - moved cmd_list, cmd_launch_deploy, cmd_launch_run)

- Moved 3 command bodies from src-tauri to skillstar-cli: `cmd_list`, `cmd_launch_deploy`, `cmd_launch_run`.
- Created `skillstar-cli/src/commands.rs` with the moved command bodies.
- Added dependencies to skillstar-cli: `skillstar-terminal`, `skillstar-core-types`, `skillstar-infra`, `serde`, `anyhow`, `serde_json`.
- `cmd_list` uses `skillstar_infra::paths::lockfile_path()` and `skillstar_core_types::lockfile::{Lockfile, LockEntry}`.
- `cmd_launch_deploy` and `cmd_launch_run` use `skillstar_terminal` functions directly (load_config, find_cli_binary, LayoutNode, generate_single_script_for_current_os, open_script_in_terminal_with_kind, etc.) and `skillstar_infra::paths::projects_manifest_path()`.
- Inlined `deploy()` logic from `terminal_backend::deploy` into `deploy_launch_config()` helper in commands.rs.
- `CliHandlers` struct updated to remove `list`, `launch_deploy`, `launch_run` fields.
- `run()` in lib.rs calls local `cmd_list()`, `cmd_launch_deploy()`, `cmd_launch_run()` directly for those commands.
- Remaining commands (install, update, create, publish, scan, doctor, pack_list, pack_remove, gui) stay in src-tauri via function pointers.
- `cmd_pack_list` and `cmd_pack_remove` could NOT be moved this slice because they depend on `PackEntry` type which lives in src-tauri's `skill_pack.rs`. Extracting PackEntry would require moving the entire PackStore/pack JSON persistence logic which is tightly coupled to src-tauri infra paths. These remain in src-tauri via function pointers.
- Verification: `cargo check -p skillstar-cli` passes, `cargo check -p skillstar` passes (53 pre-existing warnings), `cargo run -p skillstar --bin skillstar -- --help` works, `./target/debug/skillstar list` works correctly.

## T22 findings (third slice - helper functions)

- Moved 9 portable CLI helper functions from `src-tauri/src/cli.rs` to `crates/skillstar-cli/src/helpers.rs`:
  - `derive_name_hint` (pure string manipulation)
  - `resolve_installed_name` (uses skillstar_core_types::lockfile, skillstar_infra::paths, skillstar_skill_core::source_resolver)
  - `normalize_agent_ids` (pure string normalization)
  - `supported_project_agents` (uses skillstar_projects::agents::list_profiles)
  - `prompt_for_agent_selection` (uses supported_project_agents + stdin/stdout)
  - `validate_agent_ids` (uses normalize_agent_ids + supported_project_agents)
  - `resolve_rel_dirs_for_agents` (uses supported_project_agents)
  - `print_project_targets` (pure print)
  - `resolve_auto_project_agents` (uses skillstar_projects::project_manifest::detect_project_agents)
- Kept local in `src-tauri/src/cli.rs`: `install_or_reuse_skill` (depends on `skill_install::install_skill` which is app-specific), `cmd_install`, `cmd_update`, `cmd_create`, `cmd_publish`, `cmd_scan`, `cmd_doctor`, `cmd_pack_list`, `cmd_pack_remove`
- `skillstar-cli` now depends on: skillstar-core-types, skillstar-infra, skillstar-terminal, skillstar-projects, skillstar-skill-core
- Created `crates/skillstar-skill-core/` (moved from `src-tauri/crates/skill-core/`) and added to workspace members
- Updated `skillstar-skills/Cargo.toml` to use new path `../skillstar-skill-core` for the dependency
- Updated `src-tauri/Cargo.toml` to depend on `../crates/skillstar-skill-core` instead of `crates/skill-core`
- `src-tauri/src/cli.rs` now imports helpers from `skillstar_cli::{derive_name_hint, normalize_agent_ids, ...}` (re-exported via `pub use helpers::*` in skillstar-cli lib.rs)
- Removed unused imports in src-tauri/cli.rs: `source_resolver`, `project_manifest`, `agent_profile` (no longer needed after helper extraction)
- `install_or_reuse_skill` stays local because it calls `skill_install::install_skill` which is app-specific (manages in-memory SKILL_CACHE + hub symlinks)
- Verification: `cargo check -p skillstar-cli` passes, `cargo check -p skillstar` passes (56 pre-existing warnings), `cargo run -p skillstar --bin skillstar -- --help` works, `./target/debug/skillstar list` works correctly (26 skills listed).

## T22 findings (fourth slice - cmd_create moved, cmd_doctor blocked)

- Moved `cmd_create` from `src-tauri/src/cli.rs` to `crates/skillstar-cli/src/commands.rs`.
- `cmd_create` is PURE stdlib: uses only `std::env::current_dir()`, `std::fs::create_dir_all()`, `std::fs::write()`. No skillstar crate dependencies.
- Pattern followed: `cmd_create` is called directly in `skillstar_cli::run()` match arm (like `cmd_list`, `cmd_launch_deploy`, `cmd_launch_run`), NOT via `CliHandlers` function pointer.
- Removed `create: fn()` field from `CliHandlers` struct since it no longer needs a handler.
- `cmd_doctor` remains in `src-tauri/src/cli.rs` (BLOCKED): depends on `skill_pack::doctor_pack` and `skill_pack::doctor_all` which are in `src-tauri/src/core/skills/skill_pack.rs`. Per T21 findings, `skill_pack` is app-specific due to `installed_skill::invalidate_cache()` coupling. Cannot extract without larger refactor.
- `cmd_publish` (depends on `gh_manager::publish_skill`), `cmd_scan` (depends on `security_scan` + `ai_provider`), `cmd_install`/`install_or_reuse_skill` (depends on `skill_install`), `cmd_update` (depends on `lockfile` + `git_ops`), `cmd_pack_list`/`cmd_pack_remove` (depend on `skill_pack`) remain in src-tauri via `CliHandlers` function pointers.
- Verification: `cargo check -p skillstar-cli` passes, `cargo check -p skillstar` passes (56 pre-existing warnings), `cargo run -p skillstar --bin skillstar -- --help` works, `./target/debug/skillstar list` works (26 skills listed).
- Remaining in `CliHandlers`: `install`, `update`, `publish`, `scan`, `doctor`, `pack_list`, `pack_remove`, `gui` (8 handlers).

## T23 findings (app_shell extraction from lib.rs)

- Created `src-tauri/src/core/app_shell.rs` with all tray/app-shell support code extracted from lib.rs:
  - `ExitControl` struct and impl (atomic flag for exit control)
  - `TrayState` struct and impl (language state for tray)
  - `detect_system_lang()` function
  - `tray_labels()` function
  - `build_tray_menu()` function
  - `setup_tray()` function
  - `refresh_tray_menu()` function
- Added `pub mod app_shell;` to `src-tauri/src/core/mod.rs`
- `update_tray_language` command KEPT in lib.rs (composition root) but delegates to `core::app_shell::refresh_tray_menu`
- Updated call sites in `commands/patrol.rs` and `commands/models.rs` to use `crate::core::app_shell::refresh_tray_menu` and `crate::core::app_shell::ExitControl`
- Verification: `cargo check -p skillstar` passes (56 pre-existing warnings), `cargo run -p skillstar --bin skillstar -- --help` works.

## T23 (re-verification)

- T23 app_shell extraction was already completed in a prior session (learnings entries 353-366).
- `core/app_shell.rs` already existed with all required items: `ExitControl`, `TrayState`, `detect_system_lang`, `tray_labels`, `build_tray_menu`, `setup_tray`, `refresh_tray_menu`.
- `core/mod.rs` already had `pub mod app_shell;` at line 18.
- `lib.rs` already referenced `core::app_shell::*` items correctly.
- `commands/patrol.rs` and `commands/models.rs` already imported from `crate::core::app_shell`.
- Re-verified: `cargo check -p skillstar` passes, `cargo run -p skillstar --bin skillstar -- --help` works.
- No changes were needed for this session; T23 is already complete.

## T24 findings (first slice - patrol config tests)

- Added 3 inline tests to `crates/skillstar-patrol/src/config.rs`:
  - `load_config_returns_default_when_file_missing`: verifies default PatrolConfig when patrol.json absent
  - `load_config_returns_default_when_json_invalid`: verifies default when file contains invalid JSON
  - `save_load_roundtrip`: verifies save+load preserves non-default values
- Pattern used: local `test_env_lock()` + `with_temp_dir()` that sets `SKILLSTAR_DATA_DIR` to isolate filesystem/env state, following the `with_temp_home` pattern from skillstar-model-config.
- First attempt used `impl FnOnce(...)` with redundant where-clause; simplified to just `FnOnce(...)` to fix type inference error.
- `cargo test -p skillstar-patrol`: 3 passed, 0 failed
- `cargo check -p skillstar`: passes (56 pre-existing warnings unrelated to T24)

## T24 (second slice - skillstar-cli tests)

- Added 21 inline tests to `skillstar-cli` crate:
  - `lib.rs` (14 clap parse tests): `Install` (minimal, global flag, project, single agent, multiple agents comma-separated, name, all flags), `Launch::Deploy` (basic), `Launch::Run` (basic, with provider, safe mode, trailing args, provider+safe, full)
  - `helpers.rs` (7 pure function tests): `derive_name_hint` (from URL, explicit name, no slashes), `normalize_agent_ids` (empty, trims/lowercases, removes empty, sorts/dedups)
- Clap parse tests use `Cli::parse_from(["skillstar", "subcommand", ...args])` directly.
- Trailing var arg tests don't need explicit `--` separator when `allow_hyphen_values = true` is set; clap captures hyphen-prefixed tokens as positional args automatically.
- `conflicts_with` in clap means those args cannot coexist - `test_install_all_flags` initially tried `--global` + `--project` together which conflicts; fixed by removing `--project` from that test.
- Pure helper tests (`derive_name_hint`, `normalize_agent_ids`) are simple unit tests with no environment setup needed.
- `cargo test -p skillstar-cli`: 21 passed, 0 failed
- `cargo check -p skillstar`: passes (56 pre-existing warnings unrelated to T24)

## T25 findings (Source::parse redesign)

- Added `Source` struct with `repo_url` and `short` fields to `skillstar-skill-core/src/source_resolver.rs`.
- Implemented `Source::parse(input: &str) -> Result<Source>` as the primary parsing entry point.
- `normalize_repo_url` is now a compatibility shim: `let source = Source::parse(input)?; Ok((source.repo_url, source.short))`.
- `Source::parse` handles all accepted inputs: `owner/repo`, full HTTPS URLs, `.git` suffix, trailing slash, empty/whitespace errors.
- Added 8 focused `Source::parse` tests: owner/repo, full URL, .git suffix, trailing slash, owner/repo with .git, empty fails, whitespace fails, invalid format fails.
- All 25 tests in `skillstar-skill-core` pass (17 existing + 8 new).
- `cargo check -p skillstar` passes with 56 pre-existing warnings (all unrelated to T25).
- No downstream callers modified; compatibility preserved via shim delegation.

## T26 findings

- Redesigned `crates/skillstar-skill-core/src/discovery.rs` around a typed `SkillDiscovery` pipeline with `SkillDiscoveryConfig`, `DiscoveryMode`, and internal `SkillCandidate` normalization stages while keeping the legacy `discover_skills(repo_dir, full_depth)` API as a compatibility shim.
- Separated discovery into explicit phases: path selection, candidate collection, normalization, and final dedupe/sort, so root-first vs full-depth behavior is encoded in configuration instead of being spread across one mixed function.
- Extracted folder/name normalization helpers (`normalize_folder_path`, `default_skill_name`, `default_root_skill_name`) so root repo-name fallback and nested-directory naming rules remain unchanged but are easier to reason about and reuse.
- Strengthened inline discovery coverage with tests for config/full-depth mapping, compatibility parity between the new pipeline and the old API, and root candidate normalization details.

## T27 findings

- Introduced `SkillManager` trait in `src-tauri/src/core/skills/mod.rs` with three lifecycle methods: `install_skill`, `update_skill`, `uninstall_skill`.
- Added `DefaultSkillManager` (unit struct) implementing the trait by delegating to existing free functions: `skill_install::install_skill`, `skill_update::update_skill`, and the new `skill_install::uninstall_skill`.
- Extracted core `uninstall_skill(name: &str) -> Result<(), String>` from the command layer into `src-tauri/src/core/skills/skill_install.rs`, preserving full behavior: local-skill delete path, hub path removal, lockfile cleanup, project manifest cleanup, cache invalidation, and security-scan cache invalidation.
- Routed `commands/skills.rs` install/update/uninstall through `DefaultSkillManager`:
  - `install_skill` command calls `DefaultSkillManager.install_skill`
  - `uninstall_skill_sync` is now a thin adapter around `DefaultSkillManager.uninstall_skill`
  - `update_skill_sync` calls `DefaultSkillManager.update_skill` and adapts `SkillUpdateOutcome` into the existing `UpdateResult` response shape
- Removed ~80 lines of duplicated update algorithm from `commands/skills.rs`; the command layer now only contains response-building logic.
- Fixed pre-existing visibility issue in `crates/skillstar-terminal/src/provider_env.rs` (`normalize_claude_auth_keys` and `normalize_claude_model_env` were `pub(crate)` but called from `src-tauri/src/core/terminal_backend.rs`), which blocked `cargo test -p skillstar --lib` compilation. Changed both to `pub`.
- Verification: `cargo check -p skillstar` passes cleanly (only pre-existing warnings). `cargo test -p skillstar --lib` passes with 65 tests (including 4 skill_install tests).

## T28 findings

- Added `--skill` filter flag to CLI parser in `crates/skillstar-cli/src/lib.rs` with `value_delimiter = ','` so it accepts both `--skill skill1,skill2` (comma-separated) and `--skill skill1 --skill skill2` (repeatable flags).
- `CliHandlers::install` signature updated to include `skill: &[String]` parameter.
- Threaded `skill` through `cmd_install` in `src-tauri/src/cli.rs` into `install_or_reuse_skill`.
- When `--skill` filter is provided with non-empty list, `install_or_reuse_skill` routes to `skill_install::install_skills_batch` for multi-skill install; when empty, uses original single-skill `install_skill` path.
- Mutual exclusion enforced: `--name` and `--skill` cannot be used together (error returned).
- Changed `install_or_reuse_skill` return type from `(String, bool)` to `(Vec<String>, bool)` to support multiple installed skill names, with output messages updated accordingly.
- Updated all `skill_names` references throughout `cmd_install` (now a `Vec<String>`) to iterate properly for project linking and output.
- Added 4 new parser tests: single skill, comma-separated multiple, repeatable flags, combined with `--agent`.
- Verification: `cargo test -p skillstar-cli` passes (25 tests), `cargo check -p skillstar` passes (59 pre-existing warnings), `skillstar install --help` shows `--skill` flag correctly.

## T29 findings (preview mode)

- Added `--preview` flag to `Commands::Install` in `skillstar-cli/src/lib.rs` with docstring explaining it is a dry-run mode.
- `CliHandlers::install` signature updated to include `preview: bool` as the 8th parameter.
- Threaded `preview` through the `run()` match arm into `cmd_install`.
- `cmd_install` checks `preview` at entry and dispatches to `preview_install(url, name, skill)` when true, returning early before any mutations.
- `preview_install` in `src-tauri/src/cli.rs` reuses `skill_install::fetch_repo_scanned` (made `pub` to support this) to get targeting information, then prints what would happen without calling any install/lockfile/sync functions.
- `find_target_skill_preview` is a private inline helper in `cli.rs` that mirrors the exact targeting logic from `skill_install::find_target_skill` (single-skill fallback + case-insensitive match). This is intentionally a duplicate to avoid making `find_target_skill` public just for preview use.
- Preview output format: shows mode (single/batch), URL, name hint, then per-skill outcome ("would be installed", "already installed", "NOT FOUND"). When repo scan fails it gracefully falls back to "would be cloned and installed" without crashing.
- `skill_install::fetch_repo_scanned` promoted from `fn` to `pub fn` to support the preview path.
- Added 2 new parser tests: `test_install_preview_flag` and `test_install_preview_with_other_flags`.
- All 12 existing Install tests updated to include `preview` field in destructuring pattern.
- Verification: `cargo test -p skillstar-cli` passes (27 tests), `cargo check -p skillstar` passes (59 pre-existing warnings, all unrelated to T29), `skillstar install --help` shows `--preview` flag correctly.

## T30 findings (lockfile v3)

- Introduced explicit `LockfileV3` struct with `version: u32 = 3` and `skills: Vec<LockEntry>`.
- `Lockfile` is now `pub type Lockfile = LockfileV3` — backward-compatible alias so all existing call sites (`cmd_list`, `skill_install`, `skill_update`, etc.) compile unchanged.
- `LockfileV3::default()` returns `version: 3`, so missing-file now defaults to v3 (was v1).
- `load()` always upgrades the loaded version to 3 in-memory, regardless of what version was on disk (v1 or absent). No separate v1 struct needed because `LockEntry` is field-compatible with the legacy shape.
- `save()` emits `version: 3` because the struct field is always 3.
- Tests added (4 new): `missing_file_returns_version_3`, `v3_roundtrip_save_load`, `v1_style_payload_upgrades_to_v3_in_memory`, `source_folder_and_tree_hash_roundtrip`. All 16 tests pass.
- `cargo check -p skillstar` passes (59 pre-existing warnings, all unrelated to T30).

## T31 findings

- Added repo-install provenance frontmatter writing directly in `src-tauri/src/core/skills/skill_install.rs` using `markdown_translator::parser::frontmatter::{split_front_matter, render_with_front_matter}` to preserve existing body/frontmatter while merging a `provenance` mapping.
- Provenance writer records `repository_url` for all repo-cache installs and adds `source_folder` when the installed skill comes from a nested repo subpath.
- Integrated the provenance writer into both repo materialization paths: single-skill repo-cache install and batch repo-cache install, without changing install result types.
- Added 3 focused inline tests covering absent frontmatter insertion, body/frontmatter preservation, and provenance merge/update behavior.
- Verification: `lsp_diagnostics` clean on `src-tauri/src/core/skills/skill_install.rs`; `cargo test -p skillstar --lib core::skills::skill_install::tests -- --test-threads=1` passed; `cargo check -p skillstar` passed.

## T33 findings (detection taxonomy - partial)

- Implemented detection taxonomy in `crates/skillstar-security-scan/src/types.rs`:
  - `DetectionFamily` enum: Pattern, Capability, Secrets, SemanticFlow, Dynamic, ExternalTool, Policy, Other
  - `DetectionKind` struct with family + kind string and builder helpers (pattern(), secrets(), capability(), etc.)
  - `DetectionTaxonomy` struct with optional detection_kind and tags vector
- Added `taxonomy: Option<DetectionTaxonomy>` field to `StaticFinding` and `AiFinding` with serde attributes:
  - `#[serde(default, skip_serializing_if = "Option::is_none")]` ensures JSON backward compatibility
  - Existing JSON without taxonomy field deserializes correctly (defaults to None)
  - Taxonomy is skipped in JSON output when None (backward compatible for consumers)
- Implemented `Default` for both `StaticFinding` and `AiFinding` to support `..Default::default()` usage
- Added 11 tests covering taxonomy types, serde roundtrips, and finding roundtrips with/without taxonomy
- Updated all 19 StaticFinding and 7 AiFinding call sites in orchestrator.rs, scan.rs, and static_patterns.rs to include `taxonomy: None,`
- All 41 tests pass in skillstar-security-scan; cargo check -p skillstar passes

## T34 findings (capability risk detector)

- Wired taxonomy into the two `StaticFinding` constructors inside `SkillDocConsistencyAnalyzer::scan()`:
  - `taxonomy: None` → `taxonomy: Some(DetectionTaxonomy::with_kind(DetectionKind::capability(format!("contradiction-{}", rule.id_suffix))))` for contradiction findings
  - `taxonomy: None` → `taxonomy: Some(DetectionTaxonomy::with_kind(DetectionKind::capability(format!("undeclared-{}", rule.id_suffix))))` for undeclared findings
- Added `DetectionFamily`, `DetectionKind`, `DetectionTaxonomy` to `use crate::types::*` import at top of orchestrator.rs
- Added 2 new tests in orchestrator.rs `#[cfg(test)] mod tests`:
  - `doc_consistency_contradiction_finding_has_capability_taxonomy`: verifies contradiction finding has taxonomy populated with `family=Capability` and `kind` starting with "contradiction-"
  - `doc_consistency_undeclared_finding_has_capability_taxonomy`: verifies undeclared finding has taxonomy populated with `family=Capability` and `kind` starting with "undeclared-"
- Test helper `run_doc_analyzer` uses `crate::policy::resolve_policy(&crate::SecurityScanPolicy{preset:"strict",...})` to construct the policy — mirrors the passing `doc_consistency_analyzer_flags_skill_doc_contradictions` test setup exactly; bare `ResolvedSecurityScanPolicy{...}` construction caused empty findings because policy resolution applies additional preset defaults
- 43 tests pass in skillstar-security-scan; 0 failed
- cargo check -p skillstar-security-scan: passes (4 pre-existing warnings in scan.rs unrelated to T34)
- cargo check -p skillstar: passes (59 pre-existing warnings unrelated to T34)

## T35 findings (unsafe behavior detector - exhaustive surface map)

### 1. Current Unsafe-Detection Code Paths

**`static_patterns.rs`** — `PatternAnalyzer` (primary unsafe behavior surface):
18 regex-based patterns detecting inherently unsafe behaviors:

| Pattern ID | Severity | What it Detects |
|---|---|---|
| `curl_pipe_sh` | Critical | `curl ... \| sh` remote script piping |
| `wget_pipe_sh` | Critical | `wget ... \| sh` remote script piping |
| `base64_decode_exec` | High | `base64 -d \|` decode-to-exec chain |
| `eval_fetch` | Critical | `eval (fetch\|require\|import)(` dynamic remote code |
| `exec_requests` | Critical | `exec (requests.get\|post)` Python exec+HTTP |
| `reverse_shell` | Critical | `nc/ncat/netcat -e\|--exec\|-c` reverse shell |
| `bash_reverse` | Critical | `bash -i >& /dev/tcp/` bash reverse shell |
| `powershell_encoded` | Critical | PowerShell `-enc` encoded commands |
| `modify_shell_rc` | High | `> ~/.bashrc/.zshrc` shell RC persistence |
| `cron_persistence` | High | `crontab -e\|-l\|-r` cron job manipulation |
| `schtasks_persistence` | High | `schtasks /create` Windows scheduled task |
| `registry_persistence` | High | `reg add ... Run\|RunOnce` Windows registry |
| `sensitive_ssh` | High | SSH key/config file access |
| `sensitive_aws` | High | AWS credentials file access |
| `sensitive_gnupg` | High | GPG key access |
| `sensitive_env` | Medium | `.env` file reading |
| `sensitive_etc_passwd` | High | `/etc/passwd\|/etc/shadow` system files |
| `unicode_bidi` | High | Unicode bidirectional control chars |
| `long_base64` | Medium | Long base64 strings (obfuscation signal) |
| `npm_global_install` | Medium | Global npm install |
| `pip_install` | Low | Python package install |

**ALL 19 finding constructions in `static_patterns.rs` emit `taxonomy: None`** (lines 204-226).

**`orchestrator.rs` — `SemanticFlowAnalyzer`** (lines 575-990):
- `semantic_taint_local` — source+sink in same function → `taxonomy: None` (line 872-888)
- `semantic_taint_flow` — cross-function taint path → `taxonomy: None` (line 926-936)
- `semantic_entry_to_sink` — entrypoint reaches sink → `taxonomy: None` (line 971-981)

**`orchestrator.rs` — `DynamicSandboxAnalyzer`** (lines 1475-1626):
All `behavior_findings()` emissions lack taxonomy:
- `dynamic_sandbox_partial` (line 1489-1509) → `taxonomy: None`
- `dynamic_sandbox_degraded` (line 1512-1526) → `taxonomy: None`
- `dynamic_timeout` (line 1531-1543) → `taxonomy: None`
- `dynamic_non_zero_exit` (line 1548-1560) → `taxonomy: None`
- `dynamic_network_behavior` (line 1564-1570) → `taxonomy: None`
- `dynamic_exec_behavior` (line 1572-1582) → `taxonomy: None`
- `dynamic_persistence_behavior` (line 1584-1596) → `taxonomy: None`
- `dynamic_secret_access_behavior` (line 1598-1609) → `taxonomy: None`

**`orchestrator.rs` — `SecretHeuristicAnalyzer`** (lines 534-572):
All 7 secret patterns → `taxonomy: None` (line 566).

**`orchestrator.rs` — `SkillDocConsistencyAnalyzer`** (lines 209-478):
T34 implementation — ALREADY HAS taxonomy on both finding types:
- `skill_doc_contradiction_{suffix}` → `DetectionFamily::Capability` (line 442-445)
- `skill_doc_undeclared_{suffix}` → `DetectionFamily::Capability` (line 465-468)

**External tool analyzers** (lines 992-2642):
All `taxonomy: None` — semgrep, trivy, osv, grype, gitleaks, shellcheck, bandit, virustotal, sbom.

**AI chunk analysis** (`scan.rs`):
All `AiFinding` emissions → `taxonomy: None` (AI findings from prompts lack taxonomy metadata).

---

### 2. Best T35 Landing Points

**Primary: `static_patterns.rs` only**
- 19 `StaticFinding` constructions, all `taxonomy: None`
- Natural semantic fit: these patterns ARE the unsafe behaviors
- Minimal scope: single file, zero changes to orchestrator registration
- Does NOT expand into T36 (evidence trail) — no new analysis logic, just taxonomy enrichment

**Suggested taxonomy mapping for static_patterns.rs findings:**

| Pattern(s) | DetectionKind |
|---|---|
| `curl_pipe_sh`, `wget_pipe_sh` | `DetectionKind::capability("remote-script-execution")` |
| `base64_decode_exec` | `DetectionKind::capability("decode-to-exec")` |
| `eval_fetch`, `exec_requests` | `DetectionKind::capability("dynamic-code-execution")` |
| `reverse_shell`, `bash_reverse` | `DetectionKind::capability("reverse-shell")` |
| `powershell_encoded` | `DetectionKind::capability("encoded-command-execution")` |
| `modify_shell_rc`, `cron_persistence`, `schtasks_persistence`, `registry_persistence` | `DetectionKind::capability("persistence")` |
| `sensitive_ssh`, `sensitive_aws`, `sensitive_gnupg`, `sensitive_env`, `sensitive_etc_passwd` | `DetectionKind::secrets("credential-access")` |
| `unicode_bidi`, `long_base64` | `DetectionKind::pattern("obfuscation")` |
| `npm_global_install`, `pip_install` | `DetectionKind::pattern("package-install")` |

**Secondary (out of T35 scope but should get taxonomy eventually):**
- `SemanticFlowAnalyzer` 3 finding types → `DetectionFamily::SemanticFlow`
- `DynamicSandboxAnalyzer` 8 behavior types → `DetectionFamily::Dynamic`
- `SecretHeuristicAnalyzer` 7 secret types → `DetectionFamily::Secrets`

---

### 3. Existing Tests / Behavior Contract

**`types.rs` lines 532-737 — 13 tests:**
- `detection_family_serde_roundtrip` — all 8 families serialize/deserialize correctly
- `detection_family_label` — label generation matches
- `detection_kind_serde_roundtrip` — kind construction roundtrip
- `detection_kind_shorthand_constructors` — `.pattern()`, `.capability()`, `.secrets()`, etc. all work
- `detection_taxonomy_is_empty` — empty check correct
- `detection_taxonomy_serde_roundtrip` — full taxonomy roundtrip
- `static_finding_roundtrip_with_taxonomy` — finding WITH taxonomy serializes correctly
- `static_finding_roundtrip_without_taxonomy` — **backward compat**: finding WITHOUT taxonomy (old cache) deserializes to `taxonomy: None`
- `ai_finding_roundtrip_with_taxonomy` — AI finding WITH taxonomy
- `ai_finding_roundtrip_without_taxonomy` — **backward compat**: AI finding without taxonomy
- `taxonomy_skips_none_in_json` — `taxonomy: None` is skipped in JSON output

**`orchestrator.rs` lines 2644-2787 — 5 tests:**
- `dynamic_behavior_marks_partial_sandbox` — partial sandbox emits `dynamic_sandbox_partial` (line 2658)
- `dynamic_behavior_marks_missing_sandbox_as_high` — no sandbox emits `dynamic_sandbox_degraded` at High (line 2679)
- `doc_consistency_contradiction_finding_has_capability_taxonomy` — contradiction finding MUST have `DetectionFamily::Capability` with kind starting `contradiction-` (line 2735)
- `doc_consistency_undeclared_finding_has_capability_taxonomy` — undeclared finding MUST have `DetectionFamily::Capability` with kind starting `undeclared-` (line 2762)
- Plus 2 doc helper tests (`sample_script`, `doc_skill_md`, `doc_script_file`, `run_doc_analyzer`)

**`scan.rs` lines 3992-5011 — ~30 tests:**
Critical contracts verified:
- Content hash uses FULL file contents not truncated snippet (line 4089)
- Cache is scoped by scan mode (line 4112)
- Incomplete results are NOT cached (line 4135)
- Deep mode cache satisfies Smart mode (line 4153)
- `doc_consistency_analyzer_flags_skill_doc_contradictions` (line 4573)
- `static_pattern_scan_respects_policy_threshold_and_override` (line 4672)
- `meta_analyzer_dedupes_and_boosts_consensus` (line 4732)

**Behavior contracts T35 must NOT break:**
1. `static_finding_roundtrip_without_taxonomy` — old cache entries without taxonomy must deserialize cleanly
2. All 13 taxonomy tests in types.rs
3. All 5 analyzer tests in orchestrator.rs
4. All 30 scan.rs tests

---

### 4. Recommended Minimal File Scope for T35

**Only modify `crates/skillstar-security-scan/src/static_patterns.rs`**

Changes:
1. Add `DetectionTaxonomy` import to `use crate::types::{...}` at top (line 9)
2. Map each `PatternDef` group to a `DetectionKind` construction
3. Replace `taxonomy: None` with `taxonomy: Some(DetectionTaxonomy::with_kind(...))` in:
   - Line 204-213: all compiled pattern matches
   - Line 217-226: long_base64 rule

**What NOT to touch:**
- `orchestrator.rs` — no changes needed for T35 minimal scope
- `types.rs` — no new types needed; `DetectionFamily::Capability` + string kind is sufficient
- `scan.rs` — no changes
- Any external analyzer

**Verification:**
- `cargo test -p skillstar-security-scan` — all 43 tests pass
- `cargo check -p skillstar` — passes
- Backward compat: `static_finding_roundtrip_without_taxonomy` still passes

**T35 scope boundary vs T36:**
- T35 = taxonomy enrichment of existing static pattern findings
- T36 (evidence trail) = new data structures for evidence chains
- These are orthogonal; T35 does NOT require T36

## T35 implementation (unsafe behavior detector - implemented)

**Status**: Implemented and verified.

**File changed**: `crates/skillstar-security-scan/src/static_patterns.rs`

**Changes made**:
1. Added `DetectionKind` and `DetectionTaxonomy` to the `use crate::types` import
2. Added `pattern_taxonomy(pattern_id: &str) -> DetectionTaxonomy` function mapping all 19 pattern IDs + long_base64 to explicit taxonomy:
   - `curl_pipe_sh`, `wget_pipe_sh` → `DetectionFamily::Capability` / "remote-script-execution"
   - `eval_fetch`, `exec_requests` → `DetectionFamily::Capability` / "dynamic-code-execution"
   - `base64_decode_exec` → `DetectionFamily::Capability` / "decode-to-exec"
   - `reverse_shell`, `bash_reverse` → `DetectionFamily::Capability` / "reverse-shell"
   - `powershell_encoded` → `DetectionFamily::Capability` / "encoded-command-execution"
   - `modify_shell_rc`, `cron_persistence`, `schtasks_persistence`, `registry_persistence` → `DetectionFamily::Capability` / "persistence"
   - `sensitive_ssh`, `sensitive_aws`, `sensitive_gnupg`, `sensitive_env`, `sensitive_etc_passwd` → `DetectionFamily::Secrets` / "credential-access"
   - `unicode_bidi`, `long_base64` → `DetectionFamily::Pattern` / "obfuscation"
   - `npm_global_install`, `pip_install` → `DetectionFamily::Pattern` / "package-install"
3. Replaced all 19× `taxonomy: None` with `taxonomy: Some(pattern_taxonomy(pattern.id))` in the pattern match loop
4. Replaced `taxonomy: None` in the long_base64 finding with `taxonomy: Some(pattern_taxonomy("long_base64"))`
5. Added 8 new inline tests in `#[cfg(test)] mod tests`:
   - `curl_pipe_sh_has_capability_taxonomy` — verifies remote-script-execution taxonomy
   - `reverse_shell_has_capability_taxonomy` — verifies reverse-shell taxonomy
   - `sensitive_ssh_has_secrets_taxonomy` — verifies credential-access taxonomy (Secrets family)
   - `unicode_bidi_has_pattern_obfuscation_taxonomy` — verifies obfuscation taxonomy (Pattern family)
   - `npm_global_install_has_pattern_package_install_taxonomy` — verifies package-install taxonomy
   - `cron_persistence_has_capability_taxonomy` — verifies persistence taxonomy
   - `all_pattern_ids_have_taxonomy` — verifies every finding from a multi-pattern file has taxonomy
   - `long_base64_has_pattern_obfuscation_taxonomy` — verifies long_base64 gets obfuscation taxonomy

**Verification**:
- `cargo test -p skillstar-security-scan`: 51 passed, 0 failed (8 new + 43 existing)
- `cargo check -p skillstar`: passes (59 pre-existing warnings, all unrelated)
- Backward compat preserved: `static_finding_roundtrip_without_taxonomy` still passes (types.rs test)

## Research: T36-T40 External Patterns (Evidence Trail, HTML Report, Workbench Auditor, Report UI Contract)

### Official Documentation & Key Links

#### Tauri v2 File/Dialog Patterns
1. **Tauri Dialog Plugin** — `tauri_plugin_dialog` for native save dialogs
   - [Official docs](https://v2.tauri.app/plugin/dialog/)
   - [Rust API: FileDialogBuilder](https://docs.rs/tauri-plugin-dialog/latest/tauri_plugin_dialog/struct.FileDialogBuilder.html)
   - Pattern: `app.dialog().file().add_filter("HTML", &["html"]).blocking_save_file()` returns `Option<PathBuf>`
   - Frontend: `import { save } from '@tauri-apps/plugin-dialog'` then `await save({ filters: [...] })`
   - Must register plugin in `tauri.conf.json` capabilities: `dialog:allow-save`

2. **Tauri Filesystem Plugin** — `tauri_plugin_fs` for writing files
   - [Official docs](https://v2.tauri.app/plugin/file-system/)
   - Pattern: `write_text_file(path, html_content)` for saving HTML reports
   - Or Rust-side: `std::fs::write(path, html_content)`

3. **Tauri IPC Command returning data** — `invoke` from frontend
   - [Official docs](https://v2.tauri.app/develop/calling-rust/)
   - Return type must implement `serde::Serialize`
   - For large data: return `tauri::ipc::Response::new(data)` to bypass JSON serialization
   - Frontend: `invoke('generate_report')` returns Promise with result

#### Rust HTML Report Generation Patterns

4. **Provenant** (Rust scanner, ScanCode rewrite) — excellent reference for HTML + multi-format output
   - [GitHub](https://github.com/mstykow/scancode-rust) | [crates.io](https://crates.io/crates/provenant-cli)
   - Supports: JSON (ScanCode-compatible), YAML, JSON Lines, SPDX, CycloneDX, HTML, custom templates
   - Architecture separates internal domain types from output schema types (`src/output_schema/`)
   - Conversion boundary in `main.rs` converts `models::Output` → `output_schema::Output` before serialization

5. **secfinding + secreport** — Santh security ecosystem schema
   - [secfinding crate](https://crates.io/crates/secfinding/0.1.1) — canonical `Finding` struct with `Severity`, `FindingKind`, `Evidence`, `Reportable` trait
   - [secreport crate](https://crates.io/crates/secreport) — renders to SARIF, JSON, JSONL, Markdown, colored terminal
   - Key pattern: `Reportable` trait decouples finding models from rendering implementations
   - `secreport::render(&findings, Format::Sarif, "my-tool")` 

6. **cargo-audit SARIF output** — official rustsec crate
   - [sarif.rs source](https://docs.rs/cargo-audit/latest/src/cargo_audit/sarif.rs.html)
   - Converts vulnerability reports to SARIF 2.1.0 for GitHub Security tab upload
   - Key types: `SarifLog`, `Run`, `SarifResult`, `ReportingDescriptor`, `Location`

7. **oxidized-agentic-audit** — closest to SkillStar use case
   - [docs.rs](https://docs.rs/oxidized-agentic-audit/latest/oxidized_agentic_audit/)
   - Scans skill directories for prompt injection, dangerous bash, exposed secrets, unsafe packages
   - Output: JSON, SARIF, or Pretty text
   - Organized as pipeline: `output` (format) → `finding` (types) → `scan` (orchestrate) → `scanners` (trait + implementations)
   - PERFECT pattern match for SkillStar security scan architecture

8. **raxit-core schema** — Agent Assets Schema for AI agent security scans
   - [schema.rs source](https://docs.rs/raxit-core/latest/src/raxit_core/schema.rs.html)
   - Key types:
     - `ScanResult` with `manifest`, `agents`, `tools`, `models`, `memory`, `trust_boundaries`, plus finding arrays (`secret_findings`, `memory_findings`, `network_findings`, `provenance_findings`)
     - `Manifest` with `schema_version`, `subject`, `scan_id`, `scanned_at`, `scanned_by`, `scan_config`, `signature`
     - `ProvenanceFinding` with `finding_type`, `source_type`, `sink_type`, `tainted_variables`, `location`, `severity`, `data_flow`
     - `ScanConfigMetadata` with `exclude_patterns`, `parallel_workers`, `incremental`, `files_scanned`, `files_skipped`
   - Evidence trail design: each finding type has `location: SourceLocation` (file, line, end_line, function)

9. **sarif-to-md-rs** — SARIF to Markdown converter
   - [GitHub](https://github.com/fulgas/sarif-to-md-rs) | [crates.io](https://crates.io/crates/sarif-to-md-core)
   - Uses `askama` for template rendering
   - Template system in `templates/sarif/`: `report.md`, `macros.md`
   - Template has access to: runs, tool_name, tool_version, total_results, severity_counts, results

10. **rustsec report module** — official vulnerability report types
    - [report module](https://docs.rs/rustsec/latest/rustsec/report/index.html)
    - `Report` struct: `database`, `lockfile`, `settings`, `vulnerabilities`, `warnings`
    - `DatabaseInfo`, `LockfileInfo`, `VulnerabilityInfo`, `Settings` — flat info structs

#### HTML Report Templates

11. **Snyk HTML template system** — Handlebars-based
    - [snyk-to-html GitHub](https://github.com/snyk/snyk-to-html)
    - Custom templates via `-t` flag with `metadata` and `list` objects
    - Available fields: `id`, `title`, `name`, `severity`, `severityValue`, `description`, `fixedIn`, `cvssScore`, `cveSpaced`, `identifiers` (CVE/CWE/GHSA), `epssDetails`

12. **Hayabusa HTML reporter** (Rust SIEM tool)
    - [htmlreport.rs source](https://github.com/Yamato-Security/hayabusa/blob/main/src/options/htmlreport.rs)
    - Pattern: `HtmlReporter` struct with `section_order`, `md_datas` maps
    - Sections stored as Markdown data, converted to HTML at render time

---

### Recommended Patterns for SkillStar T36-T40

#### 1. Report Schema Design (T39: Report UI Contract)

**Recommended: raxit-core `ScanResult` as the primary schema pattern**

```rust
// Core report structure inspired by raxit-core schema
pub struct SecurityScanReport {
    pub manifest: ScanManifest,        // Who/what/when of scan
    pub summary: ScanSummary,          // Counts, pass/fail, severity breakdown
    pub analyzers: Vec<AnalyzerTelemetry>,  // Per-analyzer execution info
    pub findings: Vec<SecurityFinding>,      // All findings
}

pub struct ScanManifest {
    pub scan_id: String,
    pub scanned_at: String,          // RFC3339
    pub scanned_by: String,            // "skillstar-security-scan/X.Y.Z"
    pub target_path: String,
    pub scan_mode: ScanMode,          // Static, Smart, Deep
    pub files_scanned: usize,
    pub scan_config: ScanConfigMetadata,
}

pub struct AnalyzerTelemetry {
    pub analyzer_id: String,
    pub status: AnalyzerStatus,       // Success, Error, Skipped
    pub findings_count: usize,
    pub duration_ms: u64,
    pub error_message: Option<String>,
}

pub struct ScanSummary {
    pub total_findings: usize,
    pub passed: bool,
    pub severity_counts: HashMap<Severity, usize>,
    pub detection_family_counts: HashMap<DetectionFamily, usize>,
}
```

**Key insight**: Separate `manifest` (provenance), `summary` (UI display), `telemetry` (analyzer debug), and `findings` (detailed results).

#### 2. Evidence Trail (T36)

**Reference: Provenant and raxit-core provenance patterns**

Evidence trail = chain from source location → matched evidence → finding:

```rust
// Evidence chain pattern
pub struct EvidenceTrail {
    pub finding_id: String,
    pub source_location: SourceLocation,
    pub matched_content: String,       // Code snippet that triggered
    pub pattern_id: Option<String>,   // Which pattern matched
    pub confidence: Confidence,      // High, Medium, Low
    pub context: Vec<ContextLine>,    // Surrounding lines for full picture
}

pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub end_line: Option<u32>,
    pub function: Option<String>,
    pub snippet: String,              // The actual matched text
}

// Trust/provenance chain for skills
pub struct ProvenanceChain {
    pub repository_url: String,
    pub commit_sha: Option<String>,
    pub installed_path: String,
    pub installed_at: String,
    pub frontmatter_verified: bool,
}
```

#### 3. HTML Report Generator (T37)

**Recommended approach: Askama templates + serde serialization**

```rust
// Report generation in Rust
use askama::Template;

#[derive(Template)]
#[template(path = "security_report.html")]
pub struct SecurityReportTemplate {
    pub manifest: &ScanManifest,
    pub summary: &ScanSummary,
    pub findings: Vec<&SecurityFinding>,
    pub telemetry: Vec<&AnalyzerTelemetry>,
}

impl SecurityScanReport {
    pub fn to_html(&self) -> String {
        let template = SecurityReportTemplate {
            manifest: &self.manifest,
            summary: &self.summary,
            findings: self.findings.iter().collect(),
            telemetry: self.analyzers.iter().collect(),
        };
        template.render().unwrap()
    }
    
    pub fn save_html(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_html())
    }
}
```

**Template location**: `skillstar-security-scan/templates/security_report.html`

**Key sections for HTML report**:
- Summary header (scan ID, date, pass/fail badge, severity counts)
- Findings table (severity, type, file:line, description)
- Analyzer telemetry section (which analyzers ran, duration, findings count, errors)
- Evidence expansion (click to see matched code snippet)
- Raw JSON download link

#### 4. Workbench Auditor (T38)

**Reference: oxidized-agentic-audit pipeline pattern**

```rust
// skillstar-security-scan/src/workbench.rs

pub struct WorkbenchAuditor {
    pub scan_result: SecurityScanReport,
    pub evidence_trail: Vec<EvidenceTrail>,
    pub provenance_chain: Vec<ProvenanceChain>,
}

impl WorkbenchAuditor {
    /// Generate complete audit package
    pub fn generate_audit_package(&self) -> AuditPackage {
        AuditPackage {
            report: self.scan_result,
            evidence: self.evidence_trail,
            provenance: self.provenance_chain,
            exported_at: chrono::Utc::now().to_rfc3339(),
            format_version: "1.0".to_string(),
        }
    }
    
    /// Export as JSON (machine-readable)
    pub fn export_json(&self) -> String {
        serde_json::to_string_pretty(&self.generate_audit_package()).unwrap()
    }
    
    /// Export as HTML (human-readable)
    pub fn export_html(&self) -> String {
        self.scan_result.to_html()
    }
}
```

#### 5. Tauri Frontend Integration (T39)

**Recommended command pattern**:

```rust
// In skillstar-commands or src-tauri commands

#[tauri::command]
pub async fn export_security_report(
    scan_id: String,
    format: ReportFormat,  // Html, Json, Sarif
    path: Option<String>,   // If None, shows save dialog
    app_handle: AppHandle,
) -> Result<String, String> {
    let report = security_scan::get_report(&scan_id)
        .ok_or("Scan not found")?;
    
    match format {
        ReportFormat::Html => {
            let html = report.to_html();
            let save_path = if let Some(p) = path {
                PathBuf::from(p)
            } else {
                app_handle.dialog()
                    .file()
                    .add_filter("HTML", &["html"])
                    .blocking_save_file()
                    .ok_or("User cancelled")?
            };
            std::fs::write(&save_path, html).map_err(|e| e.to_string())?;
            Ok(save_path.to_string_lossy().into())
        }
        ReportFormat::Json => {
            let json = report.export_json();
            // similar save logic
            Ok(save_path)
        }
        ReportFormat::Sarif => {
            // Convert to SARIF then save
            Ok(save_path)
        }
    }
}

#[derive(Clone, Copy)]
pub enum ReportFormat {
    Html,
    Json,
    Sarif,
}
```

**Frontend TypeScript contract**:

```typescript
// src/types/security.ts
interface SecurityScanReport {
  manifest: ScanManifest;
  summary: ScanSummary;
  analyzers: AnalyzerTelemetry[];
  findings: SecurityFinding[];
}

interface ScanManifest {
  scan_id: string;
  scanned_at: string;
  scanned_by: string;
  target_path: string;
  scan_mode: 'Static' | 'Smart' | 'Deep';
  files_scanned: number;
}

interface AnalyzerTelemetry {
  analyzer_id: string;
  status: 'Success' | 'Error' | 'Skipped';
  findings_count: number;
  duration_ms: number;
  error_message?: string;
}

interface SecurityFinding {
  id: string;
  severity: 'Critical' | 'High' | 'Medium' | 'Low' | 'Info';
  taxonomy: DetectionTaxonomy | null;
  location: SourceLocation;
  title: string;
  description: string;
  evidence: string;
  recommendation?: string;
}

// API call
async function exportReport(scanId: string, format: 'Html' | 'Json' | 'Sarif') {
  return await invoke<string>('export_security_report', { 
    scanId, 
    format 
  });
}
```

---

### Anti-Patterns to Avoid

1. **Don't embed HTML generation logic in Tauri commands** — keep HTML generation in the security-scan crate; commands should only orchestrate

2. **Don't return large HTML strings via `invoke`** — use `tauri::ipc::Response` or save to file and return path

3. **Don't skip analyzer telemetry** — every analyzer should emit `AnalyzerTelemetry` even on success; critical for debugging silent failures

4. **Don't use String-based severity** — use enum with `Severity::Critical`, `Severity::High`, etc. and serialize to strings only at display layer

5. **Don't store findings without taxonomy** — T35 already added taxonomy to static findings; T36 evidence trail should extend, not replace, this

6. **Don't mix internal domain types with output types** — follow Provenant's `models::Output` → `output_schema::Output` separation pattern

7. **Don't use timestamps without timezone** — always use RFC3339 / ISO8601 with timezone

8. **Don't hardcode HTML templates in strings** — use Askama templates in separate `.html` files in `templates/` directory

---

### Portability Notes

1. **Tauri dialog plugin is cross-platform** — works on macOS, Windows, Linux, but mobile (Android/iOS) save dialogs are limited

2. **File paths** — use `PathBuf` and `Path` for cross-platform; HTML reports saved via `std::fs::write` work across all desktop platforms

3. **For Windows PowerShell launch** (Launch Deck feature): HTML report saving uses standard file I/O, no special handling needed

4. **If report is large** (>1MB HTML), consider streaming via Tauri events rather than blocking invoke


- 2026-04-23 security scan UI/report contract mapping:
  - Frontend contract hub is `src/types/index.ts` for `SecurityScanResult`, `SecurityScanEvent`, `SecurityScanEstimate`, `SecurityScanLogEntry`, `SecurityScanPolicy`, analyzer execution metadata, and finding shapes.
  - Main UI surface is `src/pages/SecurityScan.tsx`; it already loads logs, policy, estimates, exports, and renders expandable findings rows, so T39 should likely extend this page rather than introduce a parallel report page.
  - Streaming contract comes through `src/features/security/hooks/useSecurityScan.ts`, listening on `ai://security-scan` and normalizing progress/live lane state for both `SecurityScan.tsx` and sidebar/my-skills consumers.
  - Backend command surface is `src-tauri/src/commands/ai/scan.rs` + registration in `src-tauri/src/lib.rs`; exports currently return filesystem paths only, with no typed payload for HTML/Markdown preview content.
  - Reusable render patterns for rich report content already exist in `src/components/ui/Markdown.tsx`, `src/lib/markdown.tsx`, `src/components/shared/SkillReader.tsx`, `src/components/layout/DetailPanel.tsx`, and animated security visuals in `src/features/security/components/ScanFilePanel.tsx` / `RadarSweep.tsx`.
  - Likely T40 frontend tests should mirror existing Vitest style in hook/component-adjacent files using mocked Tauri `invoke`/`listen` from `src/test/setup.ts`; strong candidates are new tests beside `useSecurityScan.ts`, `SecurityScan.tsx`, or security components.


## T36 findings

- Added first-class evidence-trail contract in `crates/skillstar-security-scan/src/types.rs` via `EvidenceTrailKind`, `EvidenceTrailEntry`, and `SecurityScanResult.evidence_trail` with `#[serde(default)]` for legacy-cache compatibility.
- Evidence-trail population is deterministic and derived only from existing scan data: static findings contribute file/line/pattern/snippet/description/taxonomy, AI findings contribute file/category/evidence/recommendation/taxonomy, and analyzer execution summaries contribute analyzer id/status/findings/error.
- TypeScript contract in `src/types/index.ts` now mirrors the serialized Rust shape, including detection taxonomy metadata on findings and evidence-trail entries.
- Verification: `cargo test -p skillstar-security-scan` passed with 56 tests; `cargo check -p skillstar` passed. Remaining warnings observed were pre-existing unused-import/dead-code warnings outside the T36 scope.

## T37 findings

- Reworked `crates/skillstar-security-scan/src/scan.rs::build_html_report` from one shallow findings list into a structured self-contained report with top-level summary metrics, per-skill overview cards, analyzer execution sections, detailed static/AI finding cards, and evidence-trail rendering.
- Added local HTML helper functions in `scan.rs` (`html_badge`, metric cards, taxonomy rendering, analyzer/finding/evidence section renderers) to keep the backend-only HTML export maintainable without widening the public surface.
- HTML report now uses existing `SecurityScanResult` metadata beyond findings: `analyzer_executions`, `evidence_trail`, `meta_deduped_count`, `meta_consensus_count`, `tree_hash`, `target_language`, and scan volume counts all render into the export.
- Added focused HTML tests that assert meaningful report content rather than mere file creation: one verifies analyzer/evidence/taxonomy sections render, another verifies rich content is HTML-escaped.
- Verification: `cargo test -p skillstar-security-scan` passed (58 tests), `cargo check -p skillstar` passed. Both still emit unrelated pre-existing workspace warnings only.


## T38 findings

- Added a first-class security-scan workbench auditor surface by deriving structured audit summaries/details directly from persisted `scan-*.log` files plus nearest telemetry snapshots in `scan_telemetry.jsonl`, avoiding any new storage system.
- The parser is conservative: it recognizes stable emitted sections/fields only and records `parse_warnings` for malformed or legacy lines instead of failing the whole audit payload.
- Exposed the backend auditor through `list_security_scan_audits` and `get_security_scan_audit_detail`, with minimal TS contract additions while preserving existing raw log commands for compatibility.

## T41 findings

- Added built-in provider preset registry primitives to `skillstar-model-config::providers`, using backend-owned `ProviderEntry` payloads for claude/codex/opencode/gemini so frontend preset catalogs no longer have to be the source of truth.
- Added `get_model_provider_presets` Tauri command in `src-tauri/src/commands/models.rs` and registered it in `src-tauri/src/lib.rs` for backend-driven preset consumption.
- Added focused preset-registry tests in `crates/skillstar-model-config/src/providers.rs` covering codex presets, opencode provider-block shape/meta, and unknown-app fallback.
- Minimal frontend consumption path now uses `useProviderPresets` in `PresetCatalog` and `AddProviderDialog`; existing direct config panels still read legacy static preset files and remain a follow-up cleanup outside T41 scope.
- Verification: `cargo test -p skillstar-model-config` passed (21 tests); `cargo check -p skillstar` passed.

## T40 findings

- Added a focused frontend page test at `src/pages/SecurityScan.test.tsx` instead of waiting for T39 UI completion; it verifies the existing report/log toolbar contract with Tauri `invoke` mocks (`export_security_scan_report`, `open_folder`) and follows the repo's lightweight mock pattern.
- The `SecurityScan` page can be tested without the full provider tree by mocking `useSecurityScan` directly and stubbing `react-i18next` / `framer-motion` to deterministic lightweight implementations.
- Added one Rust markdown-report regression test in `crates/skillstar-security-scan/src/scan.rs` to cover analyzer summary rendering plus OWASP-tag emission for the verified Track B report surface; asserting on stable substrings was safer than assuming an exact heuristic tag list.
- Verification: `bun run test -- src/pages/SecurityScan.test.tsx` passed, `cargo test -p skillstar-security-scan` passed, and `cargo check -p skillstar` passed.

## T43 findings

- Implemented `crates/skillstar-model-config/src/usage_tracker.rs` as an explicit provider usage-tracking contract on top of existing quota state plus daily `provider-quota-YYYY-MM-DD.log` telemetry.
- The contract shape is `AppUsageTracker -> ProviderUsageSummary -> { current snapshot, history[] }`, reusing existing quota fields (`usage_percent`, `remaining`, `reset_time`, `plan_name`, `fetched_at`, `error`) instead of inventing a broader analytics schema.
- Historical usage is reconstructed lazily from quota logs with bounded `history_days` and `history_limit`, keeping T43 backend-first and independent from T42 health dashboard UI and T44 cloud sync.
- Added minimal Tauri command surfaces `get_provider_usage_tracker` and `get_provider_usage_summary`, giving the app a first-class usage API without expanding into frontend feature work.
- Verification: `cargo test -p skillstar-model-config usage_tracker` passed (3 tests) and `cargo check -p skillstar` passed.

## T44 findings

- Chose the smallest coherent cloud-sync scope for Track C: a versioned provider/model-state snapshot contract in `skillstar-model-config::cloud_sync` that exports/imports one app's provider registry plus existing unified provider state (`health`, `quotas`, `circuit_breakers`), without adding any remote backend or UI flow.
- The snapshot is intentionally transport-agnostic and versioned (`schema_version`, `scope`, `app_id`, `exported_at`) so later cloud transport/deep-link work can reuse the same payload instead of re-deriving state from multiple files.
- Import supports `replace` and `merge` modes, matching the current local-first architecture: `replace` fully rewrites one app slice, while `merge` preserves local entries and overlays remote ones.
- Added minimal Tauri command surfaces `export_model_cloud_sync_snapshot` and `import_model_cloud_sync_snapshot` in `src-tauri/src/commands/models.rs`, keeping T44 backend-first and independent from blocked T39/T42 UI work.
- Verification: `cargo test -p skillstar-model-config cloud_sync` passed (3 tests) and `cargo check -p skillstar` passed.

## T39 re-verification

- T39 report UI contract was already present from Track B/T40 work: `src/types/index.ts` includes detection taxonomy, analyzer execution, evidence trail, and report export types; `SecurityScan.tsx` displays analyzer execution metadata and export path; `SecurityScan.test.tsx` verifies `export_security_scan_report` invocation and last-report display.
- Verification: `lsp_diagnostics` passed for `src/types/index.ts` and `src/pages/SecurityScan.tsx`; `bun run test -- src/pages/SecurityScan.test.tsx` passed (2 tests).

## T42 findings

- Health dashboard implementation was already present across `src-tauri/src/commands/models.rs`, `src-tauri/src/lib.rs`, `src/features/models/hooks/useProviderHealthDashboard.ts`, `src/features/models/components/ProviderHealthDashboard.tsx`, and `src/features/models/components/ModelsPanel.tsx`.
- Fixed the only new frontend diagnostic in `ProviderHealthDashboard.tsx` by removing the unused `appColor` destructuring while preserving the existing prop surface and call site.
- Verification: `lsp_diagnostics` passed for `ProviderHealthDashboard.tsx`; targeted `bunx biome check src/features/models/components/ProviderHealthDashboard.tsx` passed; `bun run build` passed.

## T45 deep-link protocol findings

- Added official Tauri v2 deep-link plugin via `cargo add tauri-plugin-deep-link@2`; Cargo resolved `tauri-plugin-deep-link v2.4.7` and updated `src-tauri/Cargo.toml` plus root `Cargo.lock`.
- Configured the static desktop scheme in `src-tauri/tauri.conf.json` under `plugins.deep-link.desktop.schemes = ["skillstar"]` and added `deep-link:default` to `src-tauri/capabilities/default.json`.
- Initialized `.plugin(tauri_plugin_deep_link::init())` alongside the existing shell/dialog plugins before desktop-only updater/process plugins.
- Rust-side handling uses `DeepLinkExt`: `get_current()` covers startup/current URLs and `on_open_url()` covers runtime URLs. Windows/Linux development registration calls `register_all()`; unsupported registration failures are logged as warnings and do not break startup.
- Incoming URLs are filtered to the configured `skillstar` scheme only, then emitted to the frontend as `skillstar://deep-link` with a stable camelCase payload (`url`, `scheme`, `host`, `path`, `query`, `fragment`). No frontend dependency was needed because Rust emits the app event directly.
- Verification: `lsp_diagnostics` on `src-tauri/src/lib.rs` reported only inactive-code hints for platform cfg blocks; `cargo check -p skillstar` passes with pre-existing warnings; `bun run build` passes.

## T46 findings

- Added a compact `UnifiedProviderSwitcher` surface inside `ModelsPanel.tsx` for non-OpenCode provider lists, reusing `providers.sortedProviders`, `providers.currentId`, `providers.saving`, and `providers.switchTo` instead of adding backend state.
- The switcher shows the current provider name or an OAuth/account-auth fallback and disables the select while provider state is saving. Card-level switching remains unchanged.
- OpenCode is intentionally omitted from the active-provider switcher because `useOpenCodeNativeProviders()` models OpenCode as a pooled/native provider system with `currentId = null` and a no-op switch.
- Verification: `lsp_diagnostics` passed for `ModelsPanel.tsx`; `bunx biome check src/features/models/components/ModelsPanel.tsx` passed; `bun run build` passed.

## T47 findings

- Added Track C backend regression coverage: `skillstar-model-config` now tests Gemini preset registry shape, cloud-sync rejection for unknown app IDs, and cloud-sync rejection for unsupported schema versions.
- Added app-level deep-link helper tests in `src-tauri/src/lib.rs` for accepting the configured `skillstar://` scheme and rejecting other schemes while preserving the stable `skillstar://deep-link` event contract.
- Existing usage tracker tests already cover cached state + history aggregation, unknown provider handling, and history day-window/limit behavior; existing provider tests already cover switch-provider current/live-config behavior.
- Verification: `cargo test -p skillstar-model-config` passed (30 tests); `cargo test -p skillstar --lib deep_link_payload -- --test-threads=1` passed (2 tests); `cargo check -p skillstar` passed with pre-existing warnings; `bun run build` passed.

## T48 findings

- Added marketplace curated registry backend in `skillstar-marketplace-core`: new serializable models (`CuratedRegistryEntry`, `CuratedRegistryKind`, `CuratedRegistryUpsert`), schema version 3, `marketplace_curated_registry` table, default offline `skills.sh` seed, and list/upsert APIs using existing `with_conn` access.
- Exposed curated registry command facade in `skillstar-commands` via `list_curated_registries` and `upsert_curated_registry`, registered in `src-tauri/src/lib.rs`.
- Focused tests cover fresh schema/default seed, v2→v3 migration/user_version, and upsert/list ordering behavior.
- Verification: `cargo test -p skillstar-marketplace-core curated`, `cargo check -p skillstar-marketplace-core`, `cargo check -p skillstar-commands`, and `cargo check -p skillstar` pass.

## T49 findings

- Added marketplace multi-source provenance in `skillstar-marketplace-core` with schema version 4 and a `marketplace_skill_source_observation` table keyed by `(source_id, source_skill_id)` while keeping canonical `marketplace_skill.skill_key` behavior unchanged.
- Added serializable source observation/summary models plus list/upsert APIs. Existing skills.sh snapshot upserts now seed `skills_sh` observations without adding any new network registry fetchers.
- Exposed minimal command surfaces for `list_marketplace_source_observations` and `list_known_marketplace_sources`, and registered them in the Tauri invoke handler.
- Focused tests cover fresh v4 schema, v3→v4 migration preserving curated registries, observation upsert/list behavior, known source summaries, and canonical search compatibility.
- Verification: `cargo test -p skillstar-marketplace-core multi_source`, `cargo check -p skillstar-marketplace-core`, `cargo check -p skillstar-commands`, and `cargo check -p skillstar` pass.

## T50 findings

- Marketplace ratings/reviews use schema v5 in `skillstar-marketplace-core` and keep aggregates in `marketplace_rating_summary` separate from cached review rows in `marketplace_review`.
- Source-specific rating metadata normalizes optional source IDs with the same lowercase/dot-to-underscore convention as marketplace source observations; empty source ID represents the canonical/global aggregate.
- Minimal persistence APIs live in `snapshot.rs`: `upsert_rating_summary`, `list_rating_summaries_for_skill`, `upsert_review`, and `list_reviews_for_skill`; Tauri command wrappers are registered through `skillstar-commands` and `src-tauri/src/lib.rs`.
- Focused verification passed: `cargo test -p skillstar-marketplace-core rating`, `cargo test -p skillstar-marketplace-core review`, `cargo check -p skillstar-marketplace-core`, `cargo check -p skillstar-commands`, and `cargo check -p skillstar`.

## T52 findings

- Marketplace snapshot schema advanced to v7 with `marketplace_update_notification`, keyed by `(skill_key, source_id)` and stored in the existing marketplace.db.
- Update notification persistence is detection-only: records carry installed/available version/hash, detected/dismissed timestamps, message, and metadata; upsert clears dismissal so a newly detected candidate becomes active again.
- Source IDs reuse the T49 normalization rule (`trim` + lowercase + dot-to-underscore), while `skill_key` is canonicalized to lowercase and both required identity fields reject empty input.
- Exposed portable marketplace commands in `skillstar-commands`: upsert/list/list-for-skill/dismiss update notifications, registered in the Tauri invoke handler.
- Verification: LSP diagnostics clean for changed Rust files; `cargo test -p skillstar-marketplace-core update_notification`, `cargo check -p skillstar-marketplace-core`, `cargo check -p skillstar-commands`, and `cargo check -p skillstar` all pass.

## T53 findings

- Existing Phase 3 marketplace-core coverage already included focused tests for curated registries, multi-source observations, ratings/reviews, categories/tags, update notifications, and migration steps v2 through v7.
- Added integrated regression test `phase3_metadata_coexists_with_canonical_search_and_listing` in `snapshot.rs` to exercise curated registry, source observation, rating summary, review, category assignment, tag assignment, and update notification on the same canonical skill while verifying canonical search and leaderboard snapshots still return the expected skill.
- Verification passed from repo root: `cargo fmt --check`, `cargo test -p skillstar-marketplace-core`, `cargo check -p skillstar-marketplace-core`, `cargo check -p skillstar-commands`, and `cargo check -p skillstar`.

## T18 reconstructed evidence (F1 remediation)

**Plan terminology mismatch**: Plan T18 says "Extract skillstar-marketplace" but the actual extracted crate is `skillstar-marketplace-core`. This is restored-plan terminology (plan was recreated from memory), not an error in the implementation.

**Structural evidence**:
- Crate location: `src-tauri/crates/marketplace-core/` — published as `skillstar-marketplace-core` (Cargo.toml `name = "skillstar-marketplace-core"`).
- T21 findings (marketplace slice) added `skillstar-marketplace-core` as a dependency of `skillstar-commands` via path `../../src-tauri/crates/marketplace-core`.
- `src-tauri/src/core/marketplace_snapshot/mod.rs` is a thin shim that delegates to `skillstar_marketplace_core::snapshot` and `skillstar_marketplace_core::remote` (per T21 marketplace slice).
- `src-tauri/crates/marketplace-core/src/lib.rs` defines the crate root; `snapshot.rs`, `models.rs`, `remote.rs`, `db.rs` are the primary modules.
- T5 ("Integrate existing marketplace-core") is the origin point for the `marketplace-core` crate under `src-tauri/crates/`.
- T48-T53 marketplace work (curated registry, multi-source, ratings, categories, update notifications) all operated on `skillstar-marketplace-core` and verified the crate with `cargo test -p skillstar-marketplace-core`.

**Conclusion**: T18 extraction is represented by integration of the existing `marketplace-core` crate (named `skillstar-marketplace-core`) under `src-tauri/crates/marketplace-core/`, not a newly created `crates/skillstar-marketplace/`. The plan's naming is restored-plan terminology.

## F4 rerun scope fidelity

- After remediation, `GIT_MASTER=1 git status --short` no longer shows `?? target/`; root `.gitignore` now ignores workspace `target/` as generated Cargo output.
- The frontend files previously questioned map to explicit restored plan tasks and notepad evidence: T39/T40 security report UI contract/tests (`src/types/index.ts`, `src/lib/tauriInvoke.ts`, `src/pages/SecurityScan.test.tsx`), T42 health dashboard (`ProviderHealthDashboard*` + model command surfaces), T46 unified provider switcher (`ModelsPanel.tsx`), and T41 backend-owned provider presets (`useProviderPresets`, `AddProviderDialog`, `PresetCatalog`).

## F3 QA findings (manual smoke test)

**Date**: 2026-04-27
**Scope**: post-refactor user-surface approval gate

### Commands run (all non-destructive)
1. `cargo run -p skillstar --bin skillstar -- --help` → builds cleanly, outputs expected 10 commands (list, install, update, create, publish, scan, doctor, pack, gui, launch)
2. `cargo run -p skillstar --bin skillstar -- install --help` → shows all expected flags including new `--skill` and `--preview`
3. `./target/debug/skillstar list` → reads lockfile correctly, displays 26 installed skills with name / git URL / tree hash
4. `./target/debug/skillstar launch --help` → shows deploy + run subcommands
5. `./target/debug/skillstar pack --help` → shows list + remove subcommands
6. `./target/debug/skillstar scan --help` → shows --static-only flag
7. `./target/debug/skillstar update --help` → shows optional [NAME] argument
8. `bun run build` → Vite production build succeeds in 1.11s, all expected chunks emitted
9. `cargo test -p skillstar-cli -- --test-threads=1` → 27 passed, 0 failed
10. `cargo test -p skillstar --lib` → 70 passed, 0 failed

### Observed issues
- None blocking. Only pre-existing warnings (unused imports in shims, dead code in extracted crates, unused variables in translate commands) — all warnings, zero errors.

### Unverified
- Browser UI startup not tested because this environment lacks a display server for Tauri's WebView. Frontend `bun run build` provides the strongest feasible substitute by type-checking and bundling the entire SPA.
- Destructive CLI paths (install, update, scan on real paths, doctor) were intentionally skipped to avoid mutating `~/.skillstar`.

### Verdict
VERDICT: APPROVE
