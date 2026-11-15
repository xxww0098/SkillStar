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
    repo_url: &str,
) -> Vec<DiscoveredSkill> {
    let canonical = paths::agents_skills_root();
    let lib = crate::library::load();
    let lock = crate::skill_lock::load();
    for skill in &mut discovered {
        let same_repo = |name: &str| -> bool {
            let lock_match = lock.entry_for_folder(name).is_some_and(|(_, e)| {
                crate::source_resolver::same_remote_url(&e.source_url, repo_url)
            });
            let lib_match = lib.skills.get(name).is_some_and(|e| {
                e.source_url
                    .as_deref()
                    .is_some_and(|u| crate::source_resolver::same_remote_url(u, repo_url))
            });
            lock_match || lib_match
        };
        let matches_called_in_repo = lib.skills.iter().any(|(name, e)| {
            e.called.as_deref() == Some(skill.id.trim())
                && e.source_url
                    .as_deref()
                    .is_some_and(|u| crate::source_resolver::same_remote_url(u, repo_url))
                && canonical.join(name).symlink_metadata().is_ok()
        });
        skill.already_installed = matches_called_in_repo
            || crate::installer::resolve_canonical_skill_name(&skill.id, &skill.folder_path)
                .ok()
                .is_some_and(|(name, _)| {
                    canonical.join(&name).symlink_metadata().is_ok()
                        && (lock.skills.is_empty() || repo_url.is_empty() || same_repo(&name))
                });
    }

    skill_discover::dedupe_discovered_skills(
        discovered,
        &crate::plugin_manifest::declared_skill_dir_names(repo_dir),
    )
}
