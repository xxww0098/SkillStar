pub mod cache;
pub mod detect;
pub mod maintenance;
pub mod ops;
pub mod scan;
pub mod scan_install;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub use crate::discovery::DiscoveredSkill;
pub use crate::source_resolver::normalize_repo_url;

pub use cache::{cache_dir_name, clone_or_fetch_repo};
pub use detect::detect_new_skills_in_cached_repos;
pub use maintenance::{RepoCacheInfo, clean_unused_cache, get_cache_info};
pub use ops::{
    check_repo_skill_update_local, is_repo_cached_skill, prefetch_unique_repos,
    pull_repo_skill_update, resolve_skill_repo_root,
};
pub use scan::{compute_subtree_hash, scan_skills_in_repo};
pub use scan_install::{compute_subtree_hash_pub, install_from_repo};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub source: String,
    pub source_url: String,
    pub skills: Vec<DiscoveredSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstallTarget {
    pub id: String,
    pub folder_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoNewSkill {
    pub repo_source: String,
    pub repo_url: String,
    pub skill_id: String,
    pub folder_path: String,
    pub description: String,
}

fn upsert_repo_history_entry(_source: &str, _repo_url: &str) -> anyhow::Result<()> { Ok(()) }

pub fn scan_repo_with_mode(input: &str, full_depth: bool) -> anyhow::Result<ScanResult> {
    let (repo_url, source) = crate::source_resolver::normalize_repo_url(input)
        .context("Invalid repository URL")?;
    let repo_dir = clone_or_fetch_repo(&repo_url, &source)?;
    let skills = scan_skills_in_repo(&repo_dir, &repo_url, full_depth);
    let _ = upsert_repo_history_entry(&source, &repo_url);
    Ok(ScanResult { source, source_url: repo_url, skills })
}
