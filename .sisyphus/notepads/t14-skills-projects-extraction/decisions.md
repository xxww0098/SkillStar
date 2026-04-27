# T14 Decisions

1. **Two-module structure within crate**: `projects/` (agents + sync) and `project_manifest/` are both domain but `project_manifest` owns the disk layout (paths, index CRUD) while `projects` owns agent profiles and global sync. Both go into one crate.

2. **Re-export shims in `core/mod.rs`**: Instead of updating all 12 call sites in commands/, keep re-exports in `core/mod.rs` pointing at the new crate. This is the standard pattern already used for `skills`, `git`, etc.

3. **No new infra deps**: The crate should only depend on already-extracted crates (infra, skills, config). Do NOT add `tauri`, `reqwest`, `tokio` (use what's already in infra).

4. **Tests stay in-app**: The `#[cfg(test)]` blocks in project_manifest and agents reference `crate::core::lock_test_env()`. Keep them in the app crate; domain crate ships with doc tests only.

5. **Windows junction handling**: `agents.rs`'s `count_symlinks` already has `#[cfg(windows)]` with `crate::core::infra::fs_ops::is_link`. This pattern is portable since infra is a dep.
