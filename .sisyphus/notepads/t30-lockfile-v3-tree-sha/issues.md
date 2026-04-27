# T30 Lockfile V3 with Tree SHA - Issues/Gotchas

## 2026-04-23

## Potential Issues

### 1. No Migration Infrastructure
**Issue**: `Lockfile::load()` has no version checking or migration logic
**Impact**: T30 must implement this from scratch
**Risk**: High if old lockfile entries don't migrate cleanly

### 2. tree_hash vs tree_sha Naming
**Issue**: Current field is `tree_hash`; T30 may rename to `tree_sha` or add `tree_sha`
**Impact**: All 6 writer sites and 2+ reader sites must change
**Risk**: Medium; renaming requires careful search/replace

### 3. Sibling Hash Updates in Update Flow
**Issue**: skill_update.rs lines 83-108 update ALL siblings sharing same git_url
**Impact**: When one skill in a repo is updated, ALL skills in that repo get their tree_hash updated
**Risk**: High; this is implicit behavior that may not be intended for v3

### 4. source_folder Handling in Multi-skill Install
**Issue**: skill_pack.rs and repo_scanner/scan_install.rs handle source_folder differently
**Impact**: Inconsistency in how subfolder skills are tracked
**Risk**: Medium; may cause issues when migrating

### 5. CLI Read-Only Usage
**Issue**: `crates/skillstar-cli/src/helpers.rs` reads lockfile but field name changes cascade
**Impact**: CLI helpers must be updated if field names change
**Risk**: Low; read-only

### 6. TypeScript Skill Interface Drift
**Issue**: `src/types/index.ts` has `tree_hash: string | null` but Rust LockEntry has `tree_hash: String`
**Impact**: Potential type mismatch if frontend expects different shape
**Risk**: Low; existing code works

---

## Confirmed Working Behavior

- ✅ Lockfile save/load roundtrip works
- ✅ Version defaults to 1 when file missing
- ✅ upsert updates existing entries by name
- ✅ remove returns correct boolean
- ✅ source_folder roundtrips correctly
- ✅ Tree hash computation via gix with CLI fallback
