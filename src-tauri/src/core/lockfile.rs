//! Lockfile persistence for installed skills.
//!
//! Implementation (`LockEntry`, `Lockfile`, `get_mutex`) is in `skillstar_skill_core::lockfile`.
//! This module re-exports it and provides the app-specific `lockfile_path()` using `crate::paths`.

pub use skillstar_skill_core::lockfile::{LockEntry, Lockfile, get_mutex};

pub fn lockfile_path() -> std::path::PathBuf {
    crate::core::infra::paths::lockfile_path()
}
