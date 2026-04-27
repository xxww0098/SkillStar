use anyhow::Context;

pub use skillstar_skills::repo_scanner::{
    DiscoveredSkill, RepoCacheInfo, RepoNewSkill, ScanResult, SkillInstallTarget, cache_dir_name,
    check_repo_skill_update_local, clean_unused_cache, clone_or_fetch_repo,
    detect_new_skills_in_cached_repos, get_cache_info, is_repo_cached_skill, normalize_repo_url,
    prefetch_unique_repos, pull_repo_skill_update, resolve_skill_repo_root, scan_skills_in_repo,
};

pub(crate) mod scan_install;
pub use scan_install::{compute_subtree_hash_pub, install_from_repo};

/// Full scan flow: normalize URL → clone/fetch → scan → save history → return results.
pub fn scan_repo_with_mode(input: &str, full_depth: bool) -> anyhow::Result<ScanResult> {
    let (repo_url, source) = crate::core::git::source_resolver::normalize_repo_url(input)
        .context("Invalid repository URL")?;

    let repo_dir = clone_or_fetch_repo(&repo_url, &source)?;

    let skills = scan_skills_in_repo(&repo_dir, &repo_url, full_depth);

    let _ = crate::core::git::repo_history::upsert_entry(&source, &repo_url);

    Ok(ScanResult {
        source,
        source_url: repo_url,
        skills,
    })
}
