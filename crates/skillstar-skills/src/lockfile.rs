//! Lockfile persistence for installed skills.

pub use skillstar_core::types::lockfile::{LockEntry, Lockfile, default_lockfile_path, get_mutex};


pub fn lockfile_path() -> std::path::PathBuf {
    skillstar_core::infra::paths::lockfile_path()
}
