# T30 Lockfile V3 with Tree SHA - Research Findings

## Research Date: 2026-04-23

## 1. Schema/Type Definition Paths

### Primary Lockfile Schema (Rust - Core Types)
- **File**: `crates/skillstar-core-types/src/lockfile.rs`
  - Lines 12-20: `LockEntry` struct with `name`, `git_url`, `tree_hash`, `installed_at`, `source_folder`
  - Lines 22-26: `Lockfile` struct with `version: u32` and `skills: Vec<LockEntry>`
  - Lines 28-51: `Lockfile::load()` and `Lockfile::save()` methods
  - Lines 53-65: `Lockfile::upsert()` and `Lockfile::remove()` methods
  - Lines 76-175: Unit tests

### Re-export Chain
- `src-tauri/src/core/lockfile.rs` (lines 1-11): Re-exports from `skillstar_core_types::lockfile`
- `crates/skillstar-skill-core/src/lockfile.rs` (lines 1-5): Re-exports from `skillstar_core_types::lockfile`

### TypeScript Frontend Types
- `src/types/index.ts` lines 5-25: `Skill` interface with `tree_hash: string | null` (line 16)

### Skill Core Type (companion to lockfile)
- `crates/skillstar-core-types/src/skill.rs` lines 33-55: `Skill` struct with `tree_hash: Option<String>` (line 46)

---

## 2. Writer/Mutator Caller Paths

### A. skill_install.rs - Primary Install Flow
- **File**: `src-tauri/src/core/skills/skill_install.rs`
- Lines 157-209: `install_skill()` - constructs `LockEntry` with `tree_hash` from `git_ops::compute_tree_hash(&dest)` (line 182)
- Lines 214-295: `install_skills_batch()` - calls `install_skill()` for fallbacks
- Lines 312-346: `uninstall_skill()` - calls `lf.remove(name)`, does NOT construct new LockEntry

### B. skill_update.rs - Tree Hash Mutation
- **File**: `src-tauri/src/core/skills/skill_update.rs`
- Lines 45-152: `update_skill()` function
- Lines 83-108: For repo-cached skills, updates `sibling.tree_hash` for all siblings sharing same `git_url`
- Lines 92, 105, 111: Direct mutation of `sibling.tree_hash = tree_hash.clone()`

### C. skill_pack.rs - Pack Installation
- **File**: `src-tauri/src/core/skills/skill_pack.rs`
- Lines 300-325: Pack installation lockfile update
- Lines 308-323: Loop constructing `LockEntry` for each skill in pack
- `tree_hash` from `compute_tree_hash(repo_dir)`

### D. repo_scanner/scan_install.rs - Repo-cached Skills
- **File**: `src-tauri/src/core/skills/repo_scanner/scan_install.rs`
- Lines 77-96: LockEntry construction in `install_from_repo()`
- `tree_hash` computation (full repo or subtree)
- `source_folder` handling

### E. gh_manager.rs - Publish Flow
- **File**: `src-tauri/src/core/git/gh_manager.rs`
- Lines 507-518: LockEntry construction during `publish_skill_to_github()`
- `tree_hash` computation and LockEntry with `source_folder: Some(folder_name.to_string())`

### F. installed_skill.rs - Read Path
- **File**: `src-tauri/src/core/skills/installed_skill.rs`
- Lines 270-278: `load_lock_map()` - reads lockfile into HashMap
- Lines 372-374: Reads `tree_hash` from lock_entry for Skill construction

### G. CLI Helpers (Read-Only)
- **File**: `crates/skillstar-cli/src/helpers.rs`
- Lines 37-66: Read lockfile to resolve skill names (read-only)

### Tree Hash Computation
- **File**: `crates/skillstar-git/src/ops.rs` lines 26-68
- `compute_tree_hash()` uses `gix` library, falls back to `git rev-parse HEAD^{tree}` CLI

---

## 3. Current Tests / Compatibility Contract

### Unit Tests in lockfile.rs
- **File**: `crates/skillstar-core-types/src/lockfile.rs` lines 76-175
- `load_missing_file_returns_empty` - version defaults to 1
- `round_trip_save_load` - serialization roundtrip
- `upsert_updates_existing_entry` - verifies tree_hash update
- `remove_returns_true_when_found`
- `remove_returns_false_when_not_found`
- `upsert_with_source_folder` - source_folder roundtrip
- `save_creates_parent_directories`

### Frontend Tests
- **File**: `src/features/my-skills/hooks/useSkills.test.tsx` lines 11-63
- Mock `Skill` data with `tree_hash` fields

### Compatibility Invariants
1. **Version field**: Defaults to 1 when loading a missing file
2. **source_folder**: `Option<String>` with `skip_serializing_if`
3. **tree_hash**: Always a `String` in LockEntry
4. **NO version gating**: `load()` does NOT check or act on `version` field

---

## 4. Migration/Backward-Compat Considerations

### Legacy Path Migration (NOT Lockfile Version Migration)
- **File**: `crates/skillstar-infra/src/migration.rs` line 107
- Migrates `.skill-lock.json` → `lockfile_path()` (legacy lockfile location)

### NO Lockfile Version Migration Logic Found
- `LockFile::load()` simply deserializes JSON
- NO migration code that transforms v1 → v2 or v2 → v3 lockfile entries
- **The `version` field is read but never used for conditional logic**

### Key Implication for T30
To implement v3, you MUST add migration logic in `load()` yourself - no infrastructure exists.

---

## 5. Recommended Minimal File Scope for T30

### Must Modify (Core Schema & Logic)
1. `crates/skillstar-core-types/src/lockfile.rs` - Add new fields, update version, add migration logic, update tests
2. `src-tauri/src/core/skills/skill_install.rs` - Lines 182, 190-196: Update LockEntry construction
3. `src-tauri/src/core/skills/skill_update.rs` - Lines 92, 105, 111: Update tree_hash mutations
4. `src-tauri/src/core/skills/skill_pack.rs` - Lines 309, 317-323: Update LockEntry construction
5. `src-tauri/src/core/skills/repo_scanner/scan_install.rs` - Lines 77-96: Update LockEntry construction
6. `src-tauri/src/core/git/gh_manager.rs` - Lines 507-517: Update LockEntry construction

### Likely Need Updates (Readers)
7. `src-tauri/src/core/skills/installed_skill.rs` - Lines 372-374: Read path
8. `crates/skillstar-core-types/src/skill.rs` - Line 46: tree_hash field in Skill struct
9. `src/types/index.ts` - Line 16: tree_hash in Skill interface

### CLI Helpers
10. `crates/skillstar-cli/src/helpers.rs` - Lines 37-66: Read-only lockfile access

### Tests
11. `crates/skillstar-core-types/src/lockfile.rs` - Lines 76-175: Unit tests

### NOT Needed for V3 (Unrelated Work)
- Frontend React components (UI only displays, doesn't construct)
- Security scan cache
- Marketplace snapshot system
- Translation cache
- Project manifest/sync system
- ACP client integration

---

## Write Path Call Chain Summary

```
Tauri Command                    →   Free Function         →  Lockfile Mutation
────────────────────────────────────────────────────────────────────────────────────
commands/skills.rs:40           →   skill_install.rs:157  →  lockfile.upsert() [L190]
commands/skills.rs:81           →   skill_update.rs:45    →  tree_hash mutation [L92,105,111]
skill_install.rs:312            →   (direct)             →  lockfile.remove() [L337]
skill_pack.rs:300               →   skill_pack.rs:307     →  lockfile.upsert() [L317]
repo_scanner/scan_install.rs    →   scan_install.rs:50    →  lockfile.upsert() [L90]
gh_manager.rs:publish_skill...  →   gh_manager.rs:490     →  lockfile.upsert() [L511]
```
