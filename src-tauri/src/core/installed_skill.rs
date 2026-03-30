use super::{
    agent_profile::{self, AgentProfile},
    git_ops,
    local_skill,
    lockfile::{self, LockEntry},
    repo_scanner,
    skill::{extract_github_source_from_url, extract_skill_description, Skill, SkillCategory},
    sync,
};
use anyhow::{anyhow, Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillUpdateState {
    pub name: String,
    pub update_available: bool,
}

pub async fn list_installed_skills() -> Result<Vec<Skill>> {
    // Ensure every skill in skills-local/ has a hub symlink before scanning
    local_skill::reconcile_hub_symlinks();

    let lock_map = Arc::new(load_lock_map());
    let profiles: Arc<[AgentProfile]> = Arc::from(agent_profile::list_profiles());
    let skill_dirs = collect_skill_dirs(&sync::get_hub_skills_dir(), None)?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    let semaphore = Arc::new(Semaphore::new(skill_metadata_concurrency_limit()));
    let mut tasks = JoinSet::new();

    for path in skill_dirs {
        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };
        let lock_entry = lock_map.get(&name).cloned();
        let profiles = Arc::clone(&profiles);
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .context("Failed to acquire installed-skill metadata permit")?;

        tasks.spawn_blocking(move || {
            let _permit = permit;
            build_installed_skill(path, lock_entry, &profiles)
        });
    }

    let mut skills = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let skill = result.map_err(|err| anyhow!("installed-skill task failed: {}", err))??;
        skills.push(skill);
    }

    skills.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(skills)
}

pub async fn refresh_skill_updates(names: Option<Vec<String>>) -> Result<Vec<SkillUpdateState>> {
    let name_filter = names.map(|values| values.into_iter().collect::<HashSet<_>>());
    let skill_dirs = collect_skill_dirs(&sync::get_hub_skills_dir(), name_filter.as_ref())?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    let semaphore = Arc::new(Semaphore::new(update_check_concurrency_limit()));
    let mut tasks = JoinSet::new();

    for path in skill_dirs {
        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };

        // Skip local skills — they have no git remote to check
        if local_skill::is_local_skill(&name) {
            continue;
        }

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .context("Failed to acquire update-check permit")?;

        tasks.spawn_blocking(move || {
            let _permit = permit;
            SkillUpdateState {
                name,
                update_available: refresh_single_skill_update(&path),
            }
        });
    }

    let mut states = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let state = result.map_err(|err| anyhow!("skill-update task failed: {}", err))?;
        states.push(state);
    }

    states.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(states)
}

fn load_lock_map() -> HashMap<String, LockEntry> {
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lockfile
        .skills
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect()
}

fn collect_skill_dirs(skills_dir: &Path, names: Option<&HashSet<String>>) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(skills_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "Failed to read installed skills directory {}",
                    skills_dir.display()
                )
            })
        }
    };

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "Failed to read installed-skill entry in {}",
                skills_dir.display()
            )
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };
        if names.is_some_and(|values| !values.contains(&name)) {
            continue;
        }

        paths.push(path);
    }

    paths.sort_by(|left, right| skill_name_from_path(left).cmp(&skill_name_from_path(right)));
    Ok(paths)
}

fn build_installed_skill(
    path: PathBuf,
    lock_entry: Option<LockEntry>,
    profiles: &[AgentProfile],
) -> Result<Skill> {
    // For repo-cached skills (symlinks into .repos/), resolve the actual path
    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    if !is_repo_skill {
        let _ = git_ops::ensure_worktree_checked_out(&path);
    }

    let name = skill_name_from_path(&path).unwrap_or_default();

    // For symlinked skills, read SKILL.md from the symlink target
    let effective_path = if is_repo_skill {
        std::fs::read_link(&path)
            .map(|target| {
                if target.is_absolute() {
                    target
                } else {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .join(target)
                }
            })
            .unwrap_or_else(|_| path.clone())
    } else {
        path.clone()
    };

    let description = extract_skill_description(&effective_path);
    let tree_hash = git_ops::compute_tree_hash(&effective_path)
        .ok()
        .or_else(|| lock_entry.as_ref().map(|entry| entry.tree_hash.clone()));
    let agent_links = detect_agent_links(&name, profiles);

    // Derive source from git_url whenever possible (also works for root-level skills).
    let source = lock_entry
        .as_ref()
        .and_then(|entry| extract_github_source_from_url(&entry.git_url));

    // Determine skill type: "local" if symlink points into skills-local/
    let skill_type = if local_skill::is_local_skill(&name) {
        "local".to_string()
    } else {
        "hub".to_string()
    };

    Ok(Skill {
        name: name.clone(),
        description,
        skill_type,
        stars: 0,
        installed: true,
        update_available: false,
        last_updated: lock_entry
            .as_ref()
            .map(|entry| entry.installed_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        git_url: lock_entry
            .as_ref()
            .map(|entry| entry.git_url.clone())
            .unwrap_or_default(),
        tree_hash,
        category: SkillCategory::None,
        author: None,
        topics: Vec::new(),
        agent_links: Some(agent_links),
        rank: None,
        source,
    })
}

fn refresh_single_skill_update(path: &Path) -> bool {
    // For repo-cached skills, check update via the cached repo
    if repo_scanner::is_repo_cached_skill(path) {
        return repo_scanner::check_repo_skill_update(path);
    }
    let _ = git_ops::ensure_worktree_checked_out(path);
    git_ops::check_update(path).unwrap_or(false)
}

fn detect_agent_links(skill_name: &str, profiles: &[AgentProfile]) -> Vec<String> {
    let mut links = Vec::with_capacity(2); // most skills link to 1-2 agents
    for profile in profiles {
        let link_path = profile.global_skills_dir.join(skill_name);
        // Check both is_symlink() AND exists(): exists() follows the symlink,
        // so a broken symlink (target deleted) returns false — don't report it as linked.
        if link_path.is_symlink() && link_path.exists() {
            links.push(profile.display_name.clone());
        }
    }
    links
}

fn skill_name_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
}

fn skill_metadata_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get().clamp(2, 8))
        .unwrap_or(4)
}

fn update_check_concurrency_limit() -> usize {
    std::thread::available_parallelism()
        .map(|parallelism| parallelism.get().clamp(2, 4))
        .unwrap_or(3)
}
