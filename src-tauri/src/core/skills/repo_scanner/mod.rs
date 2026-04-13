//! Repository caching, git operations, and lockfile-backed skill install.
//!
//! ```text
//! User input (URL/owner/repo)
//!     → source_resolver  (URL normalization)
//!     → clone_or_fetch  (git cache, sparse checkout)
//!     → skill_discover  (SKILL.md scan, priority + full-depth modes)
//!     → lockfile + symlink  (install into hub)
//! ```
//!
//! Unlike [`skills::discover`](crate::core::skills::discover) (pure filesystem scan),
//! this crate enriches discovered skills with `already_installed` by consulting the lockfile.

pub(crate) mod cache;
mod detect;
mod maintenance;
pub(crate) mod ops;
pub(crate) mod scan_install;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use super::skill_discover::DiscoveredSkill;
pub use super::source_resolver::normalize_repo_url;

pub use cache::{cache_dir_name, clone_or_fetch_repo};
pub use detect::detect_new_skills_in_cached_repos;
pub use maintenance::{RepoCacheInfo, clean_unused_cache, get_cache_info};
pub use crate::core::git::ops::find_repo_root;
pub use ops::{
    check_repo_skill_update_local, is_repo_cached_skill, prefetch_unique_repos,
    pull_repo_skill_update, resolve_skill_repo_root,
};
pub use scan_install::{compute_subtree_hash_pub, install_from_repo, scan_skills_in_repo};

/// Result of scanning a GitHub repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    /// Short identifier, e.g. "vercel-labs/skills"
    pub source: String,
    /// Full URL, e.g. "https://github.com/vercel-labs/skills.git"
    pub source_url: String,
    /// All skills discovered in the repository
    pub skills: Vec<DiscoveredSkill>,
}

/// Target for batch install from scan results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallTarget {
    /// Skill directory name
    pub id: String,
    /// Relative path within the repo
    pub folder_path: String,
}

/// A new skill found in a cached repo that the user hasn't installed yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoNewSkill {
    /// Short source identifier, e.g. "jackwener/opencli"
    pub repo_source: String,
    /// Full git URL
    pub repo_url: String,
    /// Skill name (from SKILL.md frontmatter or directory name)
    pub skill_id: String,
    /// Relative path within the repo
    pub folder_path: String,
    /// Description from SKILL.md
    pub description: String,
}

/// Full scan flow: normalize URL → clone/fetch → scan → save history → return results.
pub fn scan_repo_with_mode(input: &str, full_depth: bool) -> Result<ScanResult> {
    let (repo_url, source) =
        crate::core::git::source_resolver::normalize_repo_url(input).context("Invalid repository URL")?;

    let repo_dir = clone_or_fetch_repo(&repo_url, &source)?;

    let skills = scan_skills_in_repo(&repo_dir, &repo_url, full_depth);

    let _ = crate::core::git::repo_history::upsert_entry(&source, &repo_url);

    Ok(ScanResult {
        source,
        source_url: repo_url,
        skills,
    })
}
