use crate::discovery as skill_discover;
use ss_core::infra::paths;
use std::path::Path;

use super::DiscoveredSkill;

pub fn scan_skills_in_repo(
    repo_dir: &Path,
    repo_url: &str,
    full_depth: bool,
) -> Vec<DiscoveredSkill> {
    annotate_discovered_skills(
        skill_discover::discover_skills(repo_dir, full_depth),
        repo_dir,
        repo_url,
    )
}

/// Scan only a repository-relative subtree while preserving folder paths
/// relative to the repository root for lockfile/update provenance.
pub fn scan_skills_in_repo_at(
    repo_dir: &Path,
    repo_url: &str,
    subpath: &str,
    full_depth: bool,
) -> Vec<DiscoveredSkill> {
    let discovered = skill_discover::SkillDiscovery::new(repo_dir, full_depth)
        .within(subpath)
        .discover();
    annotate_discovered_skills(discovered, repo_dir, repo_url)
}

pub(super) fn annotate_discovered_skills(
    mut discovered: Vec<DiscoveredSkill>,
    repo_dir: &Path,
    _repo_url: &str,
) -> Vec<DiscoveredSkill> {
    // D-081: provenance lives in the vercel lock; a discovered skill is
    // installed when the canonical dir for its identity exists.
    let canonical = paths::agents_skills_root();
    for skill in &mut discovered {
        skill.already_installed = crate::installer::canonical_skill_name(&skill.id)
            .is_ok_and(|name| canonical.join(name).symlink_metadata().is_ok());
    }

    skill_discover::dedupe_discovered_skills(
        discovered,
        &crate::plugin_manifest::declared_skill_dir_names(repo_dir),
    )
}
