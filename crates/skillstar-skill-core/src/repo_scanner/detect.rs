use skillstar_core_types::lockfile;
use skillstar_infra::paths;
use crate::source_resolver;

use super::RepoNewSkill;
use super::cache::cache_dir_name;
use super::scan::scan_skills_in_repo;

pub fn detect_new_skills_in_cached_repos() -> Vec<RepoNewSkill> {
    let lock_path = paths::lockfile_path();
    let lf = match lockfile::Lockfile::load(&lock_path) {
        Ok(lf) => lf,
        Err(_) => return Vec::new(),
    };

    if lf.skills.is_empty() {
        return Vec::new();
    }

    let mut repo_groups: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();

    for entry in &lf.skills {
        if entry.git_url.is_empty() {
            continue;
        }
        let norm_url = source_resolver::normalize_remote_url(&entry.git_url);
        repo_groups.entry(norm_url).or_insert_with(|| {
            let source = entry
                .git_url
                .strip_prefix("https://github.com/")
                .unwrap_or(&entry.git_url)
                .trim_end_matches(".git")
                .trim_end_matches('/')
                .to_string();
            (source, entry.git_url.clone())
        });
    }

    let cache_dir = paths::repos_cache_dir();
    let mut new_skills = Vec::new();

    for (_norm_url, (source, repo_url)) in &repo_groups {
        let repo_dir = cache_dir.join(cache_dir_name(source));
        if !repo_dir.join(".git").exists() {
            continue;
        }

        for skill in scan_skills_in_repo(&repo_dir, repo_url, false) {
            if skill.already_installed {
                continue;
            }
            new_skills.push(RepoNewSkill {
                repo_source: source.clone(),
                repo_url: repo_url.clone(),
                skill_id: skill.id,
                folder_path: skill.folder_path,
                description: skill.description,
            });
        }
    }

    new_skills
}
