//! Lockfile persistence for installed skills.
//!
//! Core implementation (`LockEntry`, `Lockfile`, `get_mutex`) is in
//! `skillstar_core_types::lockfile`. This module re-exports it and provides the
//! app-specific `lockfile_path()` using `crate::paths`.

pub use skillstar_core_types::lockfile::{LockEntry, Lockfile, get_mutex};

pub fn lockfile_path() -> std::path::PathBuf {
    crate::core::infra::paths::lockfile_path()
}
