use crate::core::{
    git_ops, installed_skill, lockfile, repo_scanner,
    skill::{
        Skill, SkillCategory, SkillType, extract_github_source_from_url, extract_skill_description,
    },
};
use std::path::Path;
use tracing::warn;

fn derive_name_hint(url: &str, name: Option<&str>) -> String {
    name.map(str::to_string).unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    })
}

fn find_target_skill<'a>(
    skills_found: &'a [repo_scanner::DiscoveredSkill],
    requested_name: Option<&str>,
    name_hint: &str,
) -> Option<&'a repo_scanner::DiscoveredSkill> {
    if let Some(name) = requested_name {
        return skills_found.iter().find(|s| s.id == name).or_else(|| {
            let lower = name.to_lowercase();
            skills_found.iter().find(|s| s.id.to_lowercase() == lower)
        });
    }

    if skills_found.len() == 1 {
        return skills_found.first();
    }

    skills_found.iter().find(|s| s.id == name_hint).or_else(|| {
        let lower = name_hint.to_lowercase();
        skills_found.iter().find(|s| s.id.to_lowercase() == lower)
    })
}

/// Compute tree hash directly from an installed skill's path.
///
/// For symlinked (repo-cached) skills this resolves the symlink target
/// and computes the hash from the real directory, avoiding a redundant
/// lockfile re-read that `install_from_repo` has just written to.
fn compute_tree_hash_for(skills_dir: &Path, installed_name: &str) -> Option<String> {
    let skill_path = skills_dir.join(installed_name);
    let effective_path = std::fs::read_link(&skill_path)
        .map(|target| {
            if target.is_absolute() {
                target
            } else {
                skill_path.parent().unwrap_or(Path::new(".")).join(target)
            }
        })
        .unwrap_or(skill_path);
    git_ops::compute_tree_hash(&effective_path).ok()
}

fn new_skill_from_install(
    name: String,
    description: String,
    git_url: String,
    tree_hash: Option<String>,
) -> Skill {
    let source = extract_github_source_from_url(&git_url);
    Skill {
        name,
        description,
        localized_description: None,
        skill_type: SkillType::Hub,
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url,
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        rank: None,
        source,
    }
}

fn try_install_from_repo_cache(
    url: &str,
    requested_name: Option<&str>,
    name_hint: &str,
    skills_dir: &Path,
) -> Option<Skill> {
    let Ok((repo_url, source)) = repo_scanner::normalize_repo_url(url) else {
        return None;
    };
    let Ok(repo_dir) = repo_scanner::clone_or_fetch_repo(&repo_url, &source) else {
        return None;
    };

    let skills_found = repo_scanner::scan_skills_in_repo(&repo_dir, &repo_url, false);
    let target = find_target_skill(&skills_found, requested_name, name_hint);

    // Guard against overwriting a local skill whose name matches the repo skill
    if let Some(skill) = &target {
        if super::paths::local_skills_dir().join(&skill.id).exists() {
            warn!(
                target: "install_skill",
                skill_id = %skill.id,
                "repo-cache skill would collide with existing local skill, skipping"
            );
            return None;
        }
    }

    let Some(skill) = target else {
        warn!(
            target: "install_skill",
            hint = %name_hint,
            found = ?skills_found.iter().map(|s| &s.id).collect::<Vec<_>>(),
            "skill not found in repo, falling back to direct clone"
        );
        return None;
    };

    let targets = vec![repo_scanner::SkillInstallTarget {
        id: skill.id.clone(),
        folder_path: skill.folder_path.clone(),
    }];

    match repo_scanner::install_from_repo(&source, &repo_url, &targets) {
        Ok(installed) if !installed.is_empty() => {
            let installed_name = installed[0].clone();
            let dest = skills_dir.join(&installed_name);
            let description = extract_skill_description(&dest);
            installed_skill::invalidate_cache();
            let tree_hash = compute_tree_hash_for(skills_dir, &installed_name);
            Some(new_skill_from_install(
                installed_name,
                description,
                repo_url,
                tree_hash,
            ))
        }
        Ok(_) => None,
        Err(err) => {
            warn!(target: "install_skill", error = %err, "repo-cache install failed, falling back");
            None
        }
    }
}

pub fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
    let skills_dir = super::paths::hub_skills_dir();
    let name_hint = derive_name_hint(&url, name.as_deref());

    if skills_dir.join(&name_hint).exists() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }
    if super::paths::local_skills_dir().join(&name_hint).exists() {
        return Err(format!(
            "Skill '{}' already exists as a local skill",
            name_hint
        ));
    }

    if let Some(skill) = try_install_from_repo_cache(&url, name.as_deref(), &name_hint, &skills_dir)
    {
        return Ok(skill);
    }

    let dest = skills_dir.join(&name_hint);
    if dest.exists() {
        return Err(format!("Skill '{}' is already installed", name_hint));
    }

    git_ops::clone_repo(&url, &dest).map_err(|e| e.to_string())?;
    let tree_hash = git_ops::compute_tree_hash(&dest).map_err(|e| e.to_string())?;

    let _lock = lockfile::get_mutex()
        .lock()
        .map_err(|_| "Lockfile mutex poisoned".to_string())?;
    let lock_path = lockfile::lockfile_path();
    let mut lockfile = lockfile::Lockfile::load(&lock_path)
        .map_err(|e| format!("Failed to load lockfile '{}': {}", lock_path.display(), e))?;
    lockfile.upsert(lockfile::LockEntry {
        name: name_hint.clone(),
        git_url: url.clone(),
        tree_hash: tree_hash.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        source_folder: None,
    });
    lockfile
        .save(&lock_path)
        .map_err(|e| format!("Failed to save lockfile '{}': {}", lock_path.display(), e))?;
    installed_skill::invalidate_cache();

    let description = extract_skill_description(&dest);
    Ok(new_skill_from_install(
        name_hint,
        description,
        url,
        Some(tree_hash),
    ))
}

/// Install multiple skills from the same repository URL in a single batch.
/// This prevents git clone/fetch overlap and lockfile serialization issues when
/// multiple skills share the same repository.
pub fn install_skills_batch(url: &str, names: &[String]) -> Result<Vec<Skill>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let skills_dir = super::paths::hub_skills_dir();
    let Ok((repo_url, source)) = repo_scanner::normalize_repo_url(url) else {
        return Err(format!("Invalid URL: {}", url));
    };

    let Ok(repo_dir) = repo_scanner::clone_or_fetch_repo(&repo_url, &source) else {
        return Err(format!("Failed to fetch repo: {}", url));
    };

    let skills_found = repo_scanner::scan_skills_in_repo(&repo_dir, &repo_url, false);

    let mut targets = Vec::new();
    let mut fallback_names = Vec::new();

    for name in names {
        // First try to find a match in the scanned repo
        let target = find_target_skill(&skills_found, Some(name), name);
        if let Some(skill) = target {
            if super::paths::local_skills_dir().join(&skill.id).exists() {
                warn!(
                    target: "install_skills_batch",
                    skill_id = %skill.id,
                    "repo-cache skill would collide with existing local skill, skipping"
                );
                continue;
            }
            if skills_dir.join(&skill.id).exists() {
                // Already installed, skip
                continue;
            }
            targets.push(repo_scanner::SkillInstallTarget {
                id: skill.id.clone(),
                folder_path: skill.folder_path.clone(),
            });
        } else {
            // Not found in repo -> fallback to direct clone path later
            fallback_names.push(name.clone());
        }
    }

    let mut installed_skills = Vec::new();

    if !targets.is_empty() {
        match repo_scanner::install_from_repo(&source, &repo_url, &targets) {
            Ok(installed) => {
                installed_skill::invalidate_cache();
                for installed_name in installed {
                    let dest = skills_dir.join(&installed_name);
                    let description = extract_skill_description(&dest);
                    let tree_hash = compute_tree_hash_for(&skills_dir, &installed_name);
                    installed_skills.push(new_skill_from_install(
                        installed_name,
                        description,
                        repo_url.clone(),
                        tree_hash,
                    ));
                }
            }
            Err(e) => {
                warn!(target: "install_skills_batch", error = %e, "batch repo install failed");
                // Fallback: all targets must be installed via direct fallback
                for t in targets {
                    fallback_names.push(t.id);
                }
            }
        }
    }

    // Process fallbacks one by one
    for name in fallback_names {
        match install_skill(url.to_string(), Some(name)) {
            Ok(skill) => installed_skills.push(skill),
            Err(e) => warn!(target: "install_skills_batch", error = %e, "fallback install failed"),
        }
    }

    Ok(installed_skills)
}

/// Install all skills from a repo that contains a skillpack.toml manifest.
/// Returns the list of installed skill names.
pub fn install_skill_pack(url: String) -> Result<Vec<String>, String> {
    let (repo_url, source) =
        super::repo_scanner::normalize_repo_url(&url).map_err(|e| format!("Invalid URL: {}", e))?;

    let repo_dir = super::repo_scanner::clone_or_fetch_repo(&repo_url, &source)
        .map_err(|e| format!("Failed to clone repo: {}", e))?;

    // Detect pack manifest
    super::skill_pack::detect_pack(&repo_dir)
        .map_err(|e| format!("Failed to detect skill pack: {}", e))?
        .ok_or_else(|| "No skillpack.toml found in repository".to_string())?;

    // Install via skill_pack module
    super::skill_pack::install_pack(&repo_dir, &source, &url)
        .map_err(|e| format!("Pack install failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::{derive_name_hint, find_target_skill};
    use crate::core::repo_scanner::DiscoveredSkill;

    fn discovered(id: &str) -> DiscoveredSkill {
        DiscoveredSkill {
            id: id.to_string(),
            folder_path: format!("skills/{id}"),
            description: String::new(),
            already_installed: false,
        }
    }

    #[test]
    fn derive_name_hint_prefers_explicit_name() {
        let hint = derive_name_hint(
            "https://github.com/example/skills.git",
            Some("explicit-name"),
        );
        assert_eq!(hint, "explicit-name");
    }

    #[test]
    fn derive_name_hint_falls_back_to_repo_tail() {
        let hint = derive_name_hint("https://github.com/example/awesome-skill.git", None);
        assert_eq!(hint, "awesome-skill");
    }

    #[test]
    fn find_target_skill_prefers_requested_name_case_insensitive() {
        let skills = vec![discovered("frontend-ui"), discovered("security-review")];
        let target = find_target_skill(&skills, Some("FRONTEND-UI"), "unused-name-hint");
        assert_eq!(target.map(|skill| skill.id.as_str()), Some("frontend-ui"));
    }

    #[test]
    fn find_target_skill_uses_single_skill_fallback() {
        let skills = vec![discovered("only-one")];
        let target = find_target_skill(&skills, None, "no-match-hint");
        assert_eq!(target.map(|skill| skill.id.as_str()), Some("only-one"));
    }
}
