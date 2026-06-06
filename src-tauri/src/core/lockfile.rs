//! Lockfile persistence for installed skills.
//!
//! Core implementation (`LockEntry`, `Lockfile`, `get_mutex`) is in
//! `skillstar_core::types::lockfile`. This module re-exports it and provides the
//! app-specific `lockfile_path()` using `crate::paths`.

pub use skillstar_core::types::lockfile::{Lockfile, get_mutex};

pub fn lockfile_path() -> std::path::PathBuf {
    skillstar_core::infra::paths::lockfile_path()
}
