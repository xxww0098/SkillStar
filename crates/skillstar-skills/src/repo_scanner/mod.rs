pub mod cache;
pub mod detect;
pub mod maintenance;
pub mod ops;
pub mod scan;
pub mod scan_install;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub use crate::discovery::DiscoveredSkill;

pub use cache::{
    cache_dir_name, cache_key_for, cached_repo_dir_if_present, clone_or_fetch_repo_at_in_session,
    clone_or_fetch_repo_in_session, existing_hub_checkout, existing_repo_cache_dir,
};
pub use detect::detect_new_skills_in_cached_repos;
pub(crate) use detect::{skill_at_revision, upstream_added_dirs};
pub use maintenance::{RepoCacheInfo, clean_unused_cache, get_cache_info};
pub use ops::pull_repo_skill_update_in_session;
pub use scan::{scan_skills_in_repo, scan_skills_in_repo_at};
pub use scan_install::{
    install_from_repo_at, install_from_repo_at_with_source_migrations, install_from_repo_in_session,
};

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
    /// Installed Skill the last update check identified this one as the
    /// successor of — the source renamed or moved it here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renamed_from: Option<String>,
}

pub(crate) fn scan_repo_with_mode_in_session(
    input: &str,
    full_depth: bool,
    session: &crate::git::transport::GitOperationSession,
) -> anyhow::Result<ScanResult> {
    let parsed = crate::source_resolver::Source::parse(input).context("Invalid repository URL")?;
    let repo_url = parsed.repo_url;
    let source = parsed.short;
    crate::skill_mutation::policy().ensure_repository_mutation_allowed(&repo_url)?;
    let repo_dir = clone_or_fetch_repo_in_session(&repo_url, &source, session)?;
    let skills = scan_skills_in_repo(&repo_dir, &repo_url, full_depth);
    Ok(ScanResult {
        source,
        source_url: repo_url,
        skills,
    })
}
