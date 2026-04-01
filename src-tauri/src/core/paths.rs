//! Centralised path resolution for all SkillStar storage locations.
//!
//! Every module that needs a filesystem path **must** go through this module
//! instead of calling `dirs::data_dir()` / `dirs::home_dir()` directly.
//!
//! ## Environment variable overrides
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `SKILLSTAR_DATA_DIR` | `~/.skillstar` | App config & metadata (JSON files) |
//! | `SKILLSTAR_HUB_DIR` | `~/.skillstar/.agents` | Skill hub, repo cache, lockfile |
//!
//! Setting these variables during development keeps dev data completely
//! separate from the production (installed) app.

use anyhow::Context;
use std::path::{Path, PathBuf};

/// App config root — JSON config files live here.
///
/// Default: `~/.skillstar/` (all platforms)
/// Override: `SKILLSTAR_DATA_DIR`
pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_DATA_DIR") {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
}

/// Hub root — skills, repo cache, lockfile, publish cache live here.
///
/// Default: `~/.skillstar/.agents`
/// Override: `SKILLSTAR_HUB_DIR`
pub fn hub_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR") {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    data_root().join(".agents")
}

/// User home directory (used for agent profile dirs like `~/.claude/skills`).
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// ── Derived Paths ──────────────────────────────────────────────────

/// `~/.skillstar/.agents/skills/` — the central skill hub.
pub fn hub_skills_dir() -> PathBuf {
    hub_root().join("skills")
}

/// `~/.skillstar/.agents/.repos/` — cached cloned repositories.
pub fn repos_cache_dir() -> PathBuf {
    hub_root().join(".repos")
}

/// `~/.skillstar/.agents/.publish-repos/<repo>/` — publish staging area.
pub fn publish_cache_dir(repo_name: &str) -> PathBuf {
    hub_root().join(".publish-repos").join(repo_name)
}

/// `~/.skillstar/.agents/skills-local/` — user-authored local skills.
pub fn local_skills_dir() -> PathBuf {
    hub_root().join("skills-local")
}

/// `~/.skillstar/.agents/.skill-lock.json`
pub fn lockfile_path() -> PathBuf {
    hub_root().join(".skill-lock.json")
}

// ── Database Paths ─────────────────────────────────────────────────

/// `~/.skillstar/marketplace.db` — local-first marketplace snapshot DB.
pub fn marketplace_db_path() -> PathBuf {
    data_root().join("marketplace.db")
}

/// `~/.skillstar/translation_cache.db` — translation cache DB.
pub fn translation_db_path() -> PathBuf {
    data_root().join("translation_cache.db")
}

/// `~/.skillstar/security_scan.db` — security scan cache DB.
pub fn security_scan_db_path() -> PathBuf {
    data_root().join("security_scan.db")
}

// ── Filesystem Utils ──────────────────────────────────────────────

/// Cross-platform symlink creation (shared utility).
///
/// All modules that need to create symlinks **must** call this function
/// instead of using `std::os::unix::fs::symlink` directly.
pub fn create_symlink(src: &Path, dst: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(src, dst)
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))?;

    Ok(())
}

// ── Helper ─────────────────────────────────────────────────────────

/// Expand a leading `~/` to the real home directory.
fn shellexpand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(rest).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}
