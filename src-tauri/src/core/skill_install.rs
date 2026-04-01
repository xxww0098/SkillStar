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

fn load_tree_hash_for(installed_name: &str) -> Option<String> {
    let lock_path = lockfile::lockfile_path();
    match lockfile::Lockfile::load(&lock_path) {
        Ok(lockfile) => lockfile
            .skills
            .iter()
            .find(|entry| entry.name == installed_name)
            .map(|entry| entry.tree_hash.clone()),
        Err(err) => {
            warn!(
                target: "install_skill",
                path = %lock_path.display(),
                error = %err,
                "failed to read lockfile for tree hash lookup"
            );
            None
        }
    }
}

fn build_installed_skill(
    name: String,
    description: String,
    git_url: String,
    tree_hash: Option<String>,
) -> Skill {
    Skill {
        name,
        description,
        localized_description: None,
        skill_type: SkillType::Hub,
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: chrono::Utc::now().to_rfc3339(),
        git_url: git_url.clone(),
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(Vec::new()),
        rank: None,
        source: extract_github_source_from_url(&git_url),
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

    let skills_found = repo_scanner::scan_skills_in_repo(&repo_dir);
    let target = find_target_skill(&skills_found, requested_name, name_hint);

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
            let tree_hash = load_tree_hash_for(&installed_name);
            Some(build_installed_skill(
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

pub async fn install_skill(url: String, name: Option<String>) -> Result<Skill, String> {
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
    Ok(build_installed_skill(
        name_hint,
        description,
        url,
        Some(tree_hash),
    ))
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
