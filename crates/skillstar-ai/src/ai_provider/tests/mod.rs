//! Unit tests for `ai_provider`, split out of the inline `#[cfg(test)] mod tests`.
//!
//! Shared helpers live here; the actual test functions are split across
//! `part1` and `part2`. Each part does `use super::*;` to pull these in along
//! with everything re-exported from the parent `ai_provider` module.

use super::*;
// Internal `pub(super)` skill-pick helpers, accessible to this test subtree
// (a descendant of `ai_provider`) but not re-exportable from `mod.rs`.
use super::skill_pick::{
    RankedSkillPickCandidate, fallback_skill_pick, parse_skill_pick_response,
    shortlist_skill_pick_candidates,
};

mod part1;
mod part2;

/// Helper: generate a unique temp dir, set env, run, restore.
/// Uses a global mutex to serialize env-var mutation across parallel tests.
pub(super) fn with_temp_data_root<F: FnOnce(&std::path::Path)>(f: F) {
    use std::sync::{LazyLock, Mutex};
    static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
    let _guard = LOCK.lock().unwrap();

    let dir = tempfile::tempdir().expect("create temp dir");
    let key = "SKILLSTAR_DATA_DIR";
    let prev = std::env::var(key).ok();
    // SAFETY: test-only, mutex-protected so no concurrent mutation.
    unsafe {
        std::env::set_var(key, dir.path());
    }
    f(dir.path());
    unsafe {
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
