# Wave 1 TDD Coverage Analysis (T8a)

## Per-Crate Test Inventory

### 1. `crates/skillstar-core-types` — 12 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/skill.rs` | 2 | `parse_skill_content`, `extract_skill_description` |
| `src/update_checker.rs` | 3 | `resolve_symlink`, `normalize_path_for_compare` |
| `src/lockfile.rs` | 7 | `LockEntry::new`, `Lockfile` serde, mutex |
| **Total** | **12** | |

**Missing:** `is_repo_cached_skill`, `is_repo_cached_skill_target_path`, `resolve_skill_repo_root`, `prefetch_unique_repos`, `check_update`, `check_update_local`, `compute_subtree_hash`

---

### 2. `crates/skillstar-infra` — 6 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/paths.rs` | 2 | `data_root` override, `hub_root` default |
| `src/util.rs` | 2 | `sha256_hex` |
| `src/fs_ops.rs` | 1 | (one fs operation) |
| `src/path_env.rs` | 1 | `path_env` |
| **Total** | **6** | |

**Missing (infra crate):** `db_pool` (0 tests — pool initialization, connection checkout/return), `migration` (0 tests), `daily_log` (0 tests), `error` (0 tests). Most path functions untested.

---

### 3. `crates/skillstar-config` — 6 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/github_mirror.rs` | 4 | URL normalization, preset lookup, env rewriting |
| `src/proxy.rs` | 2 | Proxy config parse |
| **Total** | **6** | |

**Missing:** HTTP probe/health-check path in `test_github_mirror`, `save_github_mirror_config` behavior.

---

### 4. `src-tauri/crates/marketplace-core` — 9 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/snapshot.rs` | 7 | Snapshot query/search logic |
| `src/remote.rs` | 2 | Remote fetch basics |
| **Total** | **9** | |

**Missing:** Full `InstalledSkillsFuture` integration, `SnapshotRuntimeConfig` wiring, `sync_marketplace_scope`, `resolve_skill_sources_local_first`, FTS search behavior, marketplace DB initialization.

---

### 5. `src-tauri/crates/skill-core` — 17 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/discovery.rs` | 11 | `discover_skills` root-first, full-depth, deduplication, priority ordering |
| `src/source_resolver.rs` | 6 | `normalize_repo_url`, `same_remote_url` |
| **Total** | **17** | |

**Missing:** `lockfile.rs` (0 tests — `read_lockfile`, `write_lockfile`, `merge_updates`), `shared.rs` (0 tests).

---

### 6. `src-tauri/crates/Markdown-Translator` — 47 inline tests
| File | Tests | Coverage |
|------|-------|----------|
| `src/pipeline/orchestrator.rs` | 6 | Pipeline orchestration |
| `src/pipeline/line_pipeline.rs` | 9 | Line-level fragment translation |
| `src/parser/extractor.rs` | 7 | Segment extraction |
| `src/parser/protector.rs` | 4 | Placeholder protection |
| `src/parser/mapper.rs` | 2 | Segment mapping |
| `src/parser/frontmatter.rs` | 4 | Frontmatter parsing |
| `src/validator.rs` | 10 | Output validation |
| `src/cache.rs` | 3 | Cache open/query |
| `src/agents/prompts.rs` | 2 | Prompt construction |
| **Total** | **47** | |

**Missing:** `agents/translator.rs` (0), `agents/reviewer.rs` (0), `agents/guard.rs` (0), `provider/openai.rs` (0), `provider/mod.rs` (0), `config.rs` (0), `types.rs` (0), `error.rs` (0). Markdown-Translator has the best coverage of Wave 1 but agent roles are untested.

---

## App-Side Integration (Shim) Layers from T2-T7

### Critical Shim Files (thin re-exports + wiring)

| File | Role | Tests | Gap |
|------|------|-------|-----|
| `src-tauri/src/core/skill.rs` | re-exports `skillstar_core_types::skill::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/lockfile.rs` | re-exports `skillstar_core_types::lockfile::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/path_env.rs` | re-exports `skillstar_infra::path_env` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/paths.rs` | `pub use skillstar_infra::paths::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/db_pool.rs` | `pub use skillstar_infra::db_pool::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/fs_ops.rs` | `pub use skillstar_infra::fs_ops::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/migration.rs` | `pub use skillstar_infra::migration::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/daily_log.rs` | `pub use skillstar_infra::daily_log::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/error.rs` | `pub use skillstar_infra::error::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/infra/util.rs` | `pub use skillstar_infra::util::*` | 0 | Pure re-export, no own tests |
| `src-tauri/src/core/skills/discover.rs` | `pub use skillstar_skill_core::discovery::*` | 0 | Pure re-export, no own tests |

### Active Shim Files (contain logic, not just re-exports)

| File | Role | Tests | Gap |
|------|------|-------|-----|
| `src-tauri/src/core/skills/update_checker.rs` | Uses `skillstar_core_types::` functions + git wiring | 0 | `resolve_symlink`, `is_repo_cached_skill`, `prefetch_unique_repos`, `check_update`, `check_update_local` all untested at shim level |
| `src-tauri/src/core/marketplace_snapshot/mod.rs` | Wires `skillstar_marketplace_core::snapshot` runtime config | 0 | `InstalledSkillsFuture` callback wiring, `SnapshotRuntimeConfig::new` integration, all `*_local` functions untested |
| `src-tauri/src/core/marketplace.rs` | Thin re-export + helper fns | 0 | All 15 public fns untested as a unit |
| `src-tauri/src/core/ai/mdtx_bridge.rs` | Bridges `ai_provider::` to `markdown_translator::provider::LlmProvider` | 1 (async, mock LLM) | `build_translator_config`, `restore_frontmatter_if_missing`, `split_leading_frontmatter`, error propagation paths, retry logic — all untested |

---

## Critical Integration Behaviors Untested Across Crate Boundaries

1. **`skillstar-infra::paths` → app path resolution**: `repos_cache_dir()`, `hub_skills_dir()`, `marketplace_db_path()` never tested with real `$SKILLSTAR_DATA_DIR`/`$SKILLSTAR_HUB_DIR` overrides in the context of actual usage by other crates.

2. **`skillstar-core-types::update_checker` batch prefetch flow**: The `prefetch_unique_repos` → `check_update_local` two-phase update check never tested with a real temp git repo setup at the app level.

3. **`marketplace_snapshot` + `marketplace_core::snapshot`**: The `InstalledSkillsFuture` callback that bridges `installed_skill::list_installed_skills_fast` into the snapshot runtime — never tested end-to-end. `resolve_skill_sources_local_first`, `get_leaderboard_local`, `search_local`, `get_publishers_local`, `get_publisher_repos_local`, `get_repo_skills_local`, `get_skill_detail_local`, `ai_search_local`, `sync_marketplace_scope`, `search_packs_local`, `list_packs_local` — all untested.

4. **`mdtx_bridge` → `markdown_translator`**: `SkillStarProvider::chat_json` and `chat_text` with real `ai_provider::chat_completion_capped` (not just mock). `build_translator_config` field mapping never asserted. `restore_frontmatter_if_missing` with edge cases (empty translated, no frontmatter in original, frontmatter stripped by pipeline).

5. **`skillstar_skill_core::discovery` → app**: `discover_skills` used through the app shim — never tested in context of how the app wires it (e.g., with `source_priority` behavior in app context).

6. **`skillstar_skill_core::source_resolver` → app**: `normalize_repo_url` and `same_remote_url` never tested with the app's `github_mirror` URL rewriting applied.

7. **Cross-crate SQLite contention**: `translation_cache` + `marketplace_snapshot` + `security_scan` all open SQLite DBs — no tests verify WAL mode, concurrent access, or migration ordering.

---

## No Integration Tests Whatsoever

- Zero files under `src-tauri/tests/`
- Zero files under `tests/`
- Zero `#[integration_test]` or `IntegrationTest` markers
- All testing is inline `#[test]` / `#[tokio::test]` within source files

---

## Recommended Smallest Set of New Tests for T8b

### Tier 1 — Critical Path (smoke tests for each shim)

1. **`src-tauri/src/core/skills/update_checker.rs`** — Add 4 tests:
   - `resolve_symlink_follows_relative_links`
   - `is_repo_cached_skill_returns_true_for_symlinked_skill`
   - `prefetch_unique_repos_deduplicates_by_repo_root` (mock git, verify single fetch)
   - `check_update_local_returns_none_on_failed_prefetch`

2. **`src-tauri/src/core/marketplace_snapshot/mod.rs`** — Add 3 tests:
   - `initialize_creates_snapshot_tables` (temp dir, verify schema)
   - `refresh_startup_scopes_if_needed_runs_without_panic` (mock installed skills callback)
   - `search_local_returns_empty_when_no_data`

3. **`src-tauri/src/core/ai/mdtx_bridge.rs`** — Add 3 tests:
   - `build_translator_config_maps_target_language`
   - `restore_frontmatter_if_missing_preserves_frontmatter`
   - `restore_frontmatter_if_missing_handles_empty_translated`
   - `restore_frontmatter_if_missing_no_frontmatter_in_original`

4. **`src-tauri/src/core/infra/paths.rs`** — Add 2 tests (infra-level):
   - `repos_cache_dir_under_hub_root`
   - `marketplace_db_path_in_db_dir`

### Tier 2 — Core Types Bridge (skillstar-core-types via app shim)

5. **`crates/skillstar-core-types`** — Add 4 tests:
   - `is_repo_cached_skill_target_path_within_repos_cache`
   - `resolve_skill_repo_root_finds_git_root`
   - `normalize_path_for_compare_slashes_forward`
   - `compute_subtree_hash_stable`

### Tier 3 — Config + Marketplace

6. **`crates/skillstar-config`** — Add 2 tests:
   - `test_github_mirror_probes_bundled_free_endpoint` (or mocked HTTP)
   - `github_mirror_rewrite_applies_to_github_urls_only`

7. **`src-tauri/crates/marketplace-core`** — Add 3 tests:
   - `snapshot_runtime_config_applies_paths_correctly`
   - `search_local_finds_installed_skills`
   - `ai_search_local_returns_structured_keywords`

---

## Exact Verification Command Sequence for T8c

```bash
# 1. Typecheck all Wave 1 crates
cargo check -p skillstar-core-types -p skillstar-infra -p skillstar-config \
  --manifest-path crates/

cargo check -p skillstar-marketplace-core --manifest-path src-tauri/crates/

cargo check -p skillstar-skill-core --manifest-path src-tauri/crates/

cargo check -p markdown-translator --manifest-path src-tauri/crates/

# 2. Typecheck the shim layer
cargo check --manifest-path src-tauri/

# 3. Run all tests (should be ≥ current baseline + new tests)
cargo test --workspace

# 4. Run tests per crate (verify counts)
cargo test -p skillstar-core-types -- --nocapture 2>&1 | grep "test result"
cargo test -p skillstar-infra -- --nocapture 2>&1 | grep "test result"
cargo test -p skillstar-config -- --nocapture 2>&1 | grep "test result"
cargo test -p skillstar-marketplace-core -- --nocapture 2>&1 | grep "test result"
cargo test -p skillstar-skill-core -- --nocapture 2>&1 | grep "test result"
cargo test -p markdown-translator -- --nocapture 2>&1 | grep "test result"

# 5. Run app-level shim tests
cargo test --manifest-path src-tauri/ -- core::skills::update_checker --nocapture
cargo test --manifest-path src-tauri/ -- core::marketplace_snapshot --nocapture
cargo test --manifest-path src-tauri/ -- core::ai::mdtx_bridge --nocapture

# 6. Verify no new compilation warnings in test binaries
cargo test --workspace 2>&1 | grep -E "^warning"

# 7. Quick build to verify no link errors
cargo build --workspace 2>&1 | grep -E "^error|^warning.*unresolved"
```

---

## Current Test Counts Summary

| Crate | Inline Tests |
|-------|-------------|
| `skillstar-core-types` | 12 |
| `skillstar-infra` | 6 |
| `skillstar-config` | 6 |
| `marketplace-core` | 9 |
| `skill-core` | 17 |
| `Markdown-Translator` | 47 |
| **Wave 1 Subtotal** | **97** |
| `src-tauri/src/core/*` (app shims) | ~120 (but none test cross-crate wiring) |
| **Grand Total (inline)** | **~217** |

**Zero integration tests across crate boundaries.**