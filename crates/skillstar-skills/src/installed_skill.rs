use crate::agents::{self as agent_profile, AgentProfile};
use crate::git::ops as git_ops;
use crate::lockfile::LockEntry;
pub use crate::update_state::SkillUpdateState;
use crate::{
    local_skill,
    lockfile::{self},
    repo_link, update_checker, update_state,
};
use anyhow::{Context, Result, anyhow};
use skillstar_core::types::{
    Skill, SkillCategory, extract_github_source_from_url, extract_skill_description,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

static SKILL_CACHE: LazyLock<RwLock<Option<Vec<Skill>>>> = LazyLock::new(|| RwLock::new(None));

pub fn invalidate_cache() {
    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = None;
    }
}

/// Record that a skill was just updated, so no scan can re-assert its badge.
pub fn clear_update_state(name: &str) {
    update_state::set(name, false);
}

fn normalize_snapshot_component(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn build_snapshot_skill_key(source: &str, name: &str) -> Option<String> {
    Some(format!(
        "{}/{}",
        normalize_snapshot_component(source)?,
        normalize_snapshot_component(name)?
    ))
}

pub fn installed_snapshot_markers() -> HashSet<String> {
    let mut markers = HashSet::new();

    let hub_skills_dir = skillstar_core::infra::paths::hub_skills_dir();
    if let Ok(entries) = std::fs::read_dir(&hub_skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !skillstar_core::infra::fs_ops::is_link(&path) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with('.') {
                    continue;
                }
                markers.insert(name.to_ascii_lowercase());
            }
        }
    }

    let lock_path = lockfile::lockfile_path();
    if let Ok(lockfile) = lockfile::Lockfile::load(&lock_path) {
        for entry in lockfile.skills {
            markers.insert(entry.name.to_ascii_lowercase());
            if let Some(source) = extract_github_source_from_url(&entry.git_url)
                && let Some(skill_key) = build_snapshot_skill_key(&source, &entry.name)
            {
                markers.insert(skill_key);
            }
        }
    }

    markers
}

fn apply_cached_update_states(mut skills: Vec<Skill>) -> Vec<Skill> {
    update_state::apply_to(&mut skills);
    for skill in &mut skills {
        if !matches!(
            crate::shared_channels::managed_repository_for_skill(&skill.name),
            Ok(None)
        ) {
            skill.update_available = false;
        }
    }
    skills
}

pub async fn list_installed_skills_fast() -> Result<Vec<Skill>> {
    list_installed_skills().await
}

pub async fn list_installed_skills() -> Result<Vec<Skill>> {
    if let Ok(cache) = SKILL_CACHE.read()
        && let Some(skills) = &*cache
    {
        return Ok(apply_cached_update_states(skills.clone()));
    }

    // Ensure every skill in skills-local/ has a hub symlink before scanning
    local_skill::reconcile_hub_symlinks();

    let lock_map = Arc::new(load_lock_map());
    let profiles: Arc<[AgentProfile]> = Arc::from(agent_profile::list_profiles());
    let skill_dirs = collect_skill_dirs(&skillstar_core::infra::paths::hub_skills_dir(), None)?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    let semaphore = Arc::new(Semaphore::new(skill_metadata_concurrency_limit()));

    let mut tasks = JoinSet::new();
    let skill_count = skill_dirs.len();

    for path in skill_dirs {
        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
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

    let mut skills = Vec::with_capacity(skill_count);
    while let Some(result) = tasks.join_next().await {
        let skill = result.map_err(|err| anyhow!("installed-skill task failed: {}", err))??;
        skills.push(skill);
    }

    skills.sort_by(|left, right| left.name.cmp(&right.name));

    let skills = apply_cached_update_states(skills);

    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = Some(skills.clone());
    }

    Ok(skills)
}

pub async fn refresh_skill_updates() -> Result<Vec<SkillUpdateState>> {
    let session = crate::git::transport::GitOperationSession::public();
    refresh_skill_updates_in_session(&session).await
}

pub async fn refresh_skill_updates_in_session(
    session: &crate::git::transport::GitOperationSession,
) -> Result<Vec<SkillUpdateState>> {
    // Taken before any checking starts: findings overtaken by an update that
    // lands while this scan runs are dropped when it commits.
    let scan_started = update_state::stamp();

    let skill_dirs = collect_skill_dirs(&skillstar_core::infra::paths::hub_skills_dir(), None)?
        .into_iter()
        .filter_map(|path| {
            let name = skill_name_from_path(&path)?;
            match crate::shared_channels::generic_installed_skill_is_mutable(&name, &path) {
                Ok(true) => Some(Ok(path)),
                Ok(false) => None,
                Err(error) => Some(Err(anyhow!(
                    "failed to inspect shared-channel ownership before checking updates: {error:#}"
                ))),
            }
        })
        .collect::<Result<Vec<_>>>()?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    // Pre-fetch: deduplicate repo-cached skills by repo root and fetch each
    // repo once. This avoids N redundant `git fetch` calls when N skills
    // share the same repository.
    // Returns the set of repo roots where fetch failed (e.g. shallow file
    // race). Skills in failed repos will preserve their existing update state.
    let failed_fetch_roots: Arc<std::collections::HashSet<std::path::PathBuf>> = {
        let dirs = skill_dirs.clone();
        let session = session.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _guard = crate::skill_update::acquire_update_transaction_lock()?;
            let safe_dirs = dirs
                .into_iter()
                .filter(|path| {
                    skill_name_from_path(path).is_some_and(|name| {
                        matches!(
                            crate::shared_channels::generic_installed_skill_is_mutable(&name, path),
                            Ok(true)
                        )
                    })
                })
                .collect::<Vec<_>>();
            Ok::<_, anyhow::Error>(update_checker::prefetch_unique_repos_in_session(
                &safe_dirs, &session,
            ))
        })
        .await
        .unwrap_or_else(|_| Ok(Default::default()))
        .unwrap_or_default();
        Arc::new(result)
    };

    let semaphore = Arc::new(Semaphore::new(update_check_concurrency_limit()));
    let mut tasks = JoinSet::new();
    let skill_count = skill_dirs.len();

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

        let failed_roots = Arc::clone(&failed_fetch_roots);
        let session = session.clone();
        tasks.spawn_blocking(move || {
            let _permit = permit;
            let Ok(_guard) = crate::skill_update::acquire_update_transaction_lock() else {
                return (name, None);
            };
            if !matches!(
                crate::shared_channels::generic_installed_skill_is_mutable(&name, &path),
                Ok(true)
            ) {
                return (name, None);
            }
            let update_available = refresh_single_skill_update(&path, &failed_roots, &session);
            (name, update_available)
        });
    }

    let mut states = Vec::with_capacity(skill_count);
    while let Some(result) = tasks.join_next().await {
        let (name, update_available) =
            result.map_err(|err| anyhow!("skill-update task failed: {}", err))?;
        // None means "fetch failed, status unknown" — skip so the previous
        // cached value is preserved and the UI doesn't falsely clear the
        // update badge.
        if let Some(available) = update_available {
            states.push(SkillUpdateState {
                name,
                update_available: available,
            });
        }
    }

    states.sort_by(|left, right| left.name.cmp(&right.name));
    // Skills whose fetch failed were never pushed, so they keep their previous
    // state; the rest land unless an update overtook them mid-scan.
    Ok(update_state::commit_scan(scan_started, &states))
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
            });
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

    paths.sort_by_key(|left| skill_name_from_path(left));
    Ok(paths)
}

fn build_installed_skill(
    path: PathBuf,
    lock_entry: Option<LockEntry>,
    profiles: &[AgentProfile],
) -> Result<Skill> {
    // For repo-cached skills (symlinks into .repos/), resolve the actual path
    let is_repo_skill = repo_link::is_repo_cached(&path);

    if !is_repo_skill {
        let _ = git_ops::ensure_worktree_checked_out(&path);
    }

    let name = skill_name_from_path(&path).unwrap_or_default();

    // For symlinked skills, read SKILL.md from the link target.
    let effective_path = if is_repo_skill {
        skillstar_core::infra::fs_ops::read_link_resolved(&path).unwrap_or_else(|_| path.clone())
    } else {
        path.clone()
    };

    let description = extract_skill_description(&effective_path);
    let localized_description = None;

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
        skillstar_core::types::SkillType::Local
    } else {
        skillstar_core::types::SkillType::Hub
    };

    Ok(Skill {
        name,
        description,
        localized_description,
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

fn refresh_single_skill_update(
    path: &Path,
    failed_fetch_roots: &std::collections::HashSet<std::path::PathBuf>,
    session: &crate::git::transport::GitOperationSession,
) -> Option<bool> {
    // For repo-cached skills, the repo has already been fetched by
    // prefetch_unique_repos; only compare local HEAD vs origin/HEAD.
    // Returns None when the prefetch failed for this skill's repo.
    if repo_link::is_repo_cached(path) {
        return update_checker::check_update_local(path, failed_fetch_roots);
    }
    let _ = git_ops::ensure_worktree_checked_out_in_session(path, session);
    Some(git_ops::check_update_in_session(path, session).unwrap_or(false))
}

fn detect_agent_links(skill_name: &str, profiles: &[AgentProfile]) -> Vec<String> {
    let mut links = Vec::with_capacity(2); // most skills link to 1-2 agents
    for profile in profiles {
        if !profile.has_global_skills() {
            continue;
        }
        let link_path = profile.global_skills_dir.join(skill_name);
        // Check symlinks/junctions: is_link() AND exists() (follows target — broken = false)
        if skillstar_core::infra::fs_ops::is_link(&link_path) && link_path.exists() {
            links.push(profile.display_name.clone());
        } else if link_path.is_dir() && link_path.join("SKILL.md").exists() {
            // Also detect copy-based deployment (Windows fallback)
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

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_core::types::{Skill, SkillType};

    fn test_skill(name: &str) -> Skill {
        Skill {
            name: name.to_string(),
            description: String::new(),
            localized_description: None,
            skill_type: SkillType::Hub,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: String::new(),
            git_url: String::new(),
            tree_hash: None,
            category: SkillCategory::None,
            author: None,
            topics: Vec::new(),
            agent_links: Some(Vec::new()),
            rank: None,
            source: None,
        }
    }

    #[test]
    fn listed_skills_carry_the_recorded_update_state() {
        let _guard = crate::lock_test_env();
        let temp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }
        update_state::reset_for_test();

        update_state::commit_scan(
            update_state::stamp(),
            &[
                update_state::SkillUpdateState {
                    name: "recorded-update".to_string(),
                    update_available: true,
                },
                update_state::SkillUpdateState {
                    name: "recorded-current".to_string(),
                    update_available: false,
                },
            ],
        );

        let skills =
            apply_cached_update_states(vec![test_skill("recorded-update"), test_skill("unknown")]);

        assert!(skills[0].update_available);
        assert!(
            !skills[1].update_available,
            "unscanned skills stay as built"
        );
    }
}
