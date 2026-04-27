# T30 Lockfile V3 with Tree SHA - Decisions

## 2026-04-23

## Architectural Decisions

### 1. Schema Location
**Decision**: Lockfile schema lives in `crates/skillstar-core-types/src/lockfile.rs` (LockEntry, Lockfile structs)
**Rationale**: Shared across Tauri backend and CLI helpers; skill_core re-exports it

### 2. Version Field is Currently Unused
**Decision**: The `version: u32` field exists in Lockfile but is never checked during load
**Rationale**: Legacy from early design; no migration logic exists
**Implication**: T30 MUST implement version check/migration logic in `Lockfile::load()`

### 3. Tree Hash Computation is Centralized
**Decision**: All `tree_hash` values come from `compute_tree_hash()` in `crates/skillstar-git/src/ops.rs`
**Rationale**: Consistent hashing strategy using gix library with git CLI fallback

### 4. source_folder is Optional
**Decision**: `source_folder: Option<String>` with skip_serializing_if
**Rationale**: Supports mono-repo skills where skill lives in subdirectory; None = root

### 5. Write Path is Centralized via Mutex
**Decision**: All lockfile writes go through `LOCKFILE_MTX` mutex in skill_install.rs
**Rationale**: Prevents concurrent write races; load → mutate → save pattern

### 6. No Frontend Lockfile Construction
**Decision**: Frontend never constructs LockEntry directly; only reads via Tauri invoke
**Rationale**: Backend owns persistence; frontend gets deserialized Skill objects

---

## Key Observations for V3 Implementation

1. **Migration is Greenfield**: No existing migration infrastructure; must be built from scratch
2. **Version Check Missing**: load() blindly deserializes; need to add version-gated migration
3. **6 Writer Sites**: All must be updated to use new field names/schema
4. **Read Path is Simpler**: Only installed_skill.rs and CLI helpers read lockfile
5. **Tests Already Exist**: Unit tests in lockfile.rs cover basic operations; v3 tests need addition
