//! Lockfile path helper for Tauri commands.
//!
//! Implementation lives in `skillstar_skills::lockfile`.

pub use skillstar_skills::lockfile::{Lockfile, get_mutex};

pub fn lockfile_path() -> std::path::PathBuf {
    skillstar_skills::lockfile::lockfile_path()
}
