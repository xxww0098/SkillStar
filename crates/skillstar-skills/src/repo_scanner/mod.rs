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

pub use cache::{cache_dir_name, clone_or_fetch_repo, clone_or_fetch_repo_at};
pub use detect::detect_new_skills_in_cached_repos;
pub use maintenance::{RepoCacheInfo, clean_unused_cache, get_cache_info};
pub use ops::pull_repo_skill_update;
pub use scan::{scan_skills_in_repo, scan_skills_in_repo_at};
pub use scan_install::{install_from_repo, install_from_repo_at};

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

pub fn scan_repo_with_mode(input: &str, full_depth: bool) -> anyhow::Result<ScanResult> {
    let (repo_url, source) =
        crate::source_resolver::normalize_repo_url(input).context("Invalid repository URL")?;
    let repo_dir = clone_or_fetch_repo(&repo_url, &source)?;
    let skills = scan_skills_in_repo(&repo_dir, &repo_url, full_depth);
    Ok(ScanResult {
        source,
        source_url: repo_url,
        skills,
    })
}
