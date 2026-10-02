use crate::discovery as skill_discover;
use crate::lockfile;
use crate::source_resolver;
use skillstar_core::infra::paths;
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
    let hub_skills_dir = paths::hub_skills_dir();
    let lock_entries = lockfile::Lockfile::load(&paths::lockfile_path())
        .map(|lf| lf.skills)
        .unwrap_or_default();

    for skill in &mut discovered {
        let source_folder = if skill.folder_path.is_empty() {
            None
        } else {
            Some(skill.folder_path.as_str())
        };

        let legacy_name = lock_entries.iter().find_map(|entry| {
            if source_resolver::same_remote_url(&entry.git_url, repo_url)
                && option_str_eq(entry.source_folder.as_deref(), source_folder)
            {
                Some(entry.name.clone())
            } else {
                None
            }
        });

        if let Some(name) = legacy_name {
            skill.id = name;
            // A lockfile entry for this exact source exists — that is what
            // produced `legacy_name` — so the skill is installed by definition.
            skill.already_installed = true;
        } else {
            skill.already_installed = hub_skills_dir.join(&skill.id).exists();
        }
    }

    skill_discover::dedupe_discovered_skills(
        discovered,
        &crate::plugin_manifest::declared_skill_dir_names(repo_dir),
    )
}

fn option_str_eq(left: Option<&str>, right: Option<&str>) -> bool {
    left == right
}
