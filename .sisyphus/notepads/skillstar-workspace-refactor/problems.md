- Open follow-up after T2 cleanup: repo root `target/` is not ignored by `.gitignore`, so verification commands can reintroduce untracked build artifacts until ignore rules are broadened in a later scoped task.
- Open follow-up after T3: the workspace still contains broader app crates that depend on infra through compatibility shims; the next extraction step should rewire direct dependencies gradually to reduce shim surface.

- T6 follow-up: `cargo test -p skillstar` still fails due to pre-existing missing test helpers (`get_mymemory_de`, `mymemory_translate_short_text`) in the translation domain. This is outside T6 scope and does not block `cargo check -p skillstar`, which passes cleanly.
- T8 follow-up: `cargo test -p skillstar --lib` remains blocked by the same pre-existing translation test-helper failures. All new Wave 1 test code compiles cleanly; the blocker is in `src-tauri/src/core/ai_provider/mod.rs` test module, outside T8 scope.

- T9 follow-up: `gh_manager.rs` still mixes pure GH CLI behavior with app-specific publish flow (lockfile, infra paths, fs_ops). A future extraction could graduate pure GH CLI status/helpers into `skillstar-git` while keeping publish workflow in `src-tauri`.
- `repo_history.rs` and `dismissed_skills.rs` are small and could be moved with path-injected APIs, but they were intentionally retained in `src-tauri` to keep T9 scoped to `ops.rs` as the plan requires.

- T10 follow-up: `cargo test -p skillstar --lib` remains blocked by pre-existing translation test-helper failures in `ai_provider/mod.rs`. All new model-config tests pass in the extracted crate (`cargo test -p skillstar-model-config`: 8 passed). No new problems introduced by T10.


## T11 follow-up

- Translation-specific orchestration (translate_text, summarize_text, etc.) currently lives in skillstar-ai/src/ai_provider/mod.rs along with the reusable chat primitives. T15 should further refine this boundary by extracting translation orchestration into a dedicated skillstar-translation crate or module.
- The ai_provider/mod.rs in skillstar-ai is still a ~2500-line monolith. Future refactoring could split it into smaller modules within skillstar-ai (chat.rs, prompt.rs, lang.rs, config_persist.rs, provider_ref.rs, translation.rs) as originally attempted.
- Prompt file paths in skillstar-ai use relative paths crossing into src-tauri/ (../../../../src-tauri/prompts/). A cleaner approach would be to copy or symlink prompt files into crates/skillstar-ai/prompts/.


## T12 follow-up

- `detect_short_text_source_lang` CJK classification heuristic conflates Japanese and Chinese for multi-character pure CJK input. This is pre-existing behavior and outside Wave 2 scope, but could be refined in a future translation-quality pass.

## T13 follow-up

- `src-tauri/src/core/skills/discover.rs` is now a pure re-export shim with no consumers in src-tauri. It could be removed entirely if no external code depends on `crate::core::skills::discover`, but keeping it preserves backward compatibility.
- `update_checker.rs` in src-tauri has many unused functions now that `ops.rs` calls `skillstar_core_types::update_checker` directly. A future cleanup could remove or simplify the src-tauri `update_checker` shim.
- `installed_skill.rs`, `local_skill.rs`, `skill_update.rs`, `skill_bundle.rs`, `skill_pack.rs`, and `skill_install.rs` remain app-coupled. A future extraction (`skillstar-projects`, `skillstar-translation`) might allow some of these to move cleanly.

## T14 follow-up

- `src-tauri/src/core/project_manifest/mod.rs` is a hybrid shim: pure re-exports plus one app-coupled function (`import_scanned_skills`). If `local_skill` is ever extracted into a reusable crate, `import_scanned_skills` could move into `skillstar-projects` and the shim could become a pure one-line re-export.
- No new problems introduced by T14.

## T15 follow-up

- No new problems introduced by T15.
- `cargo test -p skillstar --lib` passes cleanly (90 tests). `cargo test -p skillstar-translation` passes cleanly (12 tests).
- The unused re-export warnings in the `translation_api/mod.rs` shim could be suppressed with `#[allow(unused_imports)]` if desired, but they are pre-existing style warnings, not errors.
- `mdtx_bridge.rs` remains in `src-tauri` and depends on both `skillstar-ai` and `markdown-translator`. A future refactoring could consider whether it belongs in `skillstar-translation` or a separate integration crate, but the current boundary is clean.

## T16 follow-up

- `skill_md_chain_runs_mdtx_pipeline_with_mock_llm` flakiness should be investigated if it becomes a CI blocker. Hypothesis: shared temp file (`__skill__.md`) or SQLite cache (`mdtx_cache.db`) between concurrent tests causes race conditions. A fix would involve making the temp file path unique per test or disabling cache in tests.
- `mymemory.rs` `get_mymemory_de` and `get_mymemory_de_async` remain untested because they perform filesystem I/O and UUID generation. To test them deterministically would require either dependency injection or temp-dir isolation with `tempfile`, which was deemed too invasive for this TDD pass.
- `mdtx_bridge.rs` `SkillStarProvider::chat_json`/`chat_text` retry logic remains untested because it calls `ai_provider::chat_completion_capped` directly (not through a trait). Testing retry paths would require refactoring the bridge to accept a completion trait, which is outside the minimal-test scope.
- No new problems introduced by T16. `cargo check -p skillstar` remains clean (only pre-existing warnings). `cargo test -p skillstar-translation --lib`: 51 passed. `cargo test -p skillstar --lib`: 107 passed.


## T41 follow-up

- Frontend config panels (`ClaudeConfigPanel`, `CodexConfigPanel`) and OpenCode native-provider metadata mapping still read static preset modules; a later task can fully remove frontend preset duplication once all consumers switch to backend registry data.
