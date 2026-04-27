# Wave 1 TDD Issues & Blockers

## Critical Issues

### 1. Zero Cross-Crate Integration Tests
All tests are isolated inline `#[test]` blocks within single modules. No test exercises the boundary between:
- `skillstar-infra::paths` → `skillstar-core-types::update_checker`
- `skillstar-core-types` → `skillstar-marketplace-core::snapshot`
- `marketplace-core` → `markdown-translator` (via `mdtx_bridge`)
- Any crate → `skillstar_skill_core::discovery`

### 2. Shim Layers Are Pure Re-Exports With No Own Tests
11 of 15 shim files in `src-tauri/src/core/` are `pub use crate::*` with zero own tests. While these are thin, their wiring logic (e.g., `SnapshotRuntimeConfig::new` with callbacks) contains behavior that needs tests.

### 3. `db_pool` Has Zero Tests
The SQLite connection pool is infrastructure-critical but has no tests for:
- Pool initialization
- Connection checkout/return
- Pool configuration parameters

### 4. `migration` Module Has Zero Tests
Schema migrations run on every startup but have no regression tests.

### 5. `InstalledSkillsFuture` Callback Never Tested
`marketplace_snapshot/mod.rs` wires a callback that returns installed skills from the app's `installed_skill` module. This callback wiring has never been exercised in isolation.

### 6. `mdtx_bridge` Retry/Timeout Logic Untested
`SkillStarProvider::chat_json` and `chat_text` implement retry with exponential backoff and per-call timeouts (45s). These error paths are only exercised by the single `skill_md_chain_runs_mdtx_pipeline_with_mock_llm` test which uses a mock that always succeeds.

### 7. All 17 `skillstar-skill-core` Tests Are Isolated to Single Crate
`snapshot.rs` (7 tests), `remote.rs` (2 tests) test marketplace-core in isolation. The `InstalledSkillsFuture` wiring to the app's `installed_skill` module is never tested as a composed unit.

### 8. Frontmatter Restoration Edge Cases Untested
`restore_frontmatter_if_missing` in `mdtx_bridge.rs` has 4 edge cases but only one is covered by the single async test.

## Gotchas

- `#[cfg(test)]` modules in shim files sometimes hide test-only imports that would fail in non-test builds — `mdtx_bridge.rs` line 431 is an example (`use markdown_translator::error::Error as MdtxError` in `#[cfg(test)]`).
- `tempfile` is a `dev-dependency` in all Wave 1 crates — tests that need temp dirs must use it.
- `skillstar-infra` edition 2024 uses `rust-version = 1.94.1` for the tauri crates but edition 2024 for the standalone crates — test binary compatibility depends on matching Rust versions.
- `markdown_translator::cache::TranslationCache::open` creates the DB schema on first open — no explicit migration test exists for the cache schema.
- No `#[ignore]` tests are tracked — there's no mechanism to mark known-missing coverage as acknowledged technical debt.