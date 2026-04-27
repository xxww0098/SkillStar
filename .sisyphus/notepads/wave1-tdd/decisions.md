# Wave 1 TDD — Architectural Decisions

## Decision 1: Inline Tests vs Separate Integration Test Files

**Decision:** All Wave 1 tests are inline `#[test]` / `#[tokio::test]` within source files. No separate `tests/` directories.

**Rationale:** Matches existing project convention. Inline tests are colocated with the code they test, making them easier to maintain during refactoring (T2-T7 extracted crates in-place).

**Implication for T8b:** New integration tests that cross crate boundaries should probably live in a new `src-tauri/tests/integration/` directory (which currently does not exist) rather than being forced into inline modules. This needs a workspace-level `[[test]]` entry in the Tauri Cargo.toml or a separate integration test crate.

---

## Decision 2: Shim Layer Strategy

**Decision:** App-side shim modules in `src-tauri/src/core/*/` are primarily `pub use crate::*` thin re-exports. Active shims (`update_checker.rs`, `marketplace_snapshot/mod.rs`, `mdtx_bridge.rs`) add wiring logic on top of the re-exports.

**Rationale:** T2-T7 preserved the original module structure while pointing implementations at the extracted crates. The shims exist because the app has its own app-specific wiring (callbacks, git command construction, path overrides) that can't live in the extracted crate.

**Implication for T8b:** Tests for shim behavior should live in the shim file itself (inline `#[cfg(test)]`), not in the underlying crate. This validates the wiring/glue logic specifically.

---

## Decision 3: Mock Strategy for External Dependencies

**Decision:** `mdtx_bridge.rs` uses a hand-rolled `JsonEchoMock` struct implementing `LlmProvider` for testing. Marketplace-core tests use temp directories. No `mockall` or similar framework is used in Wave 1 crates.

**Implication for T8b:** Continue hand-rolled mocks for `LlmProvider`, `MarkdownFragmentTranslator`, etc. Do not introduce `mockall` just for T8b — it would be a new dependency for the project.

---

## Decision 4: SQLite Test Strategy

**Decision:** Tests that need SQLite use `tempfile::TempDir` and set `SKILLSTAR_DATA_DIR` to the temp path. `translation_cache.rs` has a `with_temp_data_root` helper that manages env var isolation.

**Implication for T8b:** Follow the same `with_temp_data_root` / temp dir pattern for any new SQLite-related tests. Do not use in-memory SQLite for integration tests — real file-based SQLite with WAL is the production configuration.