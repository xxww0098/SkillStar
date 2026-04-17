use super::{
    agent_profile::{self, AgentProfile},
    git_ops, local_skill,
    lockfile::{self, LockEntry},
    repo_scanner,
    skill::{Skill, SkillCategory, extract_github_source_from_url, extract_skill_description},
    translation_cache::CachedTranslation,
};
use anyhow::{Context, Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

static SKILL_CACHE: LazyLock<RwLock<Option<Vec<Skill>>>> = LazyLock::new(|| RwLock::new(None));
static UPDATE_STATE_CACHE: LazyLock<RwLock<HashMap<String, bool>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn invalidate_cache() {
    if let Ok(mut cache) = SKILL_CACHE.write() {
        *cache = None;
    }
}

pub fn clear_update_state(name: &str) {
    if let Ok(mut cache) = UPDATE_STATE_CACHE.write() {
        // Assume false since we just updated it, rather than deleting entirely
        // which could cause a flash if the UI forces a refresh before the next update check
        cache.insert(name.to_string(), false);
    }
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

    let hub_skills_dir = crate::core::infra::paths::hub_skills_dir();
    if let Ok(entries) = std::fs::read_dir(&hub_skills_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !crate::core::infra::fs_ops::is_link(&path) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                markers.insert(name.to_ascii_lowercase());
            }
        }
    }

    let lock_path = lockfile::lockfile_path();
    if let Ok(lockfile) = lockfile::Lockfile::load(&lock_path) {
        for entry in lockfile.skills {
            markers.insert(entry.name.to_ascii_lowercase());
            if let Some(source) = extract_github_source_from_url(&entry.git_url) {
                if let Some(skill_key) = build_snapshot_skill_key(&source, &entry.name) {
                    markers.insert(skill_key);
                }
            }
        }
    }

    markers
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillUpdateState {
    pub name: String,
    pub update_available: bool,
}

fn apply_cached_update_states(mut skills: Vec<Skill>) -> Vec<Skill> {
    if let Ok(update_states) = UPDATE_STATE_CACHE.read() {
        for skill in &mut skills {
            if let Some(&update_available) = update_states.get(&skill.name) {
                skill.update_available = update_available;
            }
        }
    }
    skills
}

pub async fn list_installed_skills_fast() -> Result<Vec<Skill>> {
    list_installed_skills().await
}

pub async fn list_installed_skills() -> Result<Vec<Skill>> {
    if let Ok(cache) = SKILL_CACHE.read() {
        if let Some(skills) = &*cache {
            return Ok(apply_cached_update_states(skills.clone()));
        }
    }

    // Ensure every skill in skills-local/ has a hub symlink before scanning
    local_skill::reconcile_hub_symlinks();

    let lock_map = Arc::new(load_lock_map());
    let profiles: Arc<[AgentProfile]> = Arc::from(agent_profile::list_profiles());
    let skill_dirs = collect_skill_dirs(&crate::core::infra::paths::hub_skills_dir(), None)?;

    if skill_dirs.is_empty() {
        return Ok(Vec::new());
    }

    let semaphore = Arc::new(Semaphore::new(skill_metadata_concurrency_limit()));
    let mut target_language = None;
    let config = crate::core::ai_provider::load_config_async().await;
    if !config.target_language.is_empty() {
        target_language = Some(config.target_language);
    }

    // Batch-preload all short translations in a single SQL query.
    // This replaces N individual DB lookups during concurrent skill building.
    let preloaded_translations: Arc<HashMap<String, CachedTranslation>> =
        if let Some(ref lang) = target_language {
            let lang_clone = lang.clone();
            let map = tokio::task::spawn_blocking(move || {
                crate::core::ai::translation_cache::preload_short_translations(&lang_clone)
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            Arc::new(map)
        } else {
            Arc::new(HashMap::new())
        };

    let mut tasks = JoinSet::new();
    let skill_count = skill_dirs.len();

    for path in skill_dirs {
        let Some(name) = skill_name_from_path(&path) else {
            continue;
        };
        let lock_entry = lock_map.get(&name).cloned();
        let profiles = Arc::clone(&profiles);
        let target_lang = target_language.clone();
        let translations = Arc::clone(&preloaded_translations);
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .context("Failed to acquire installed-skill metadata permit")?;

        tasks.spawn_blocking(move || {
            let _permit = permit;
            build_installed_skill(
                path,
                lock_entry,
                &profiles,
                target_lang.as_deref(),
                &translations,
            )
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

pub async fn refresh_skill_updates(names: Option<Vec<String>>) -> Result<Vec<SkillUpdateState>> {
    let name_filter = names.map(|values| values.into_iter().collect::<HashSet<_>>());
    let skill_dirs = collect_skill_dirs(
        &crate::core::infra::paths::hub_skills_dir(),
        name_filter.as_ref(),
    )?;

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
        let result =
            tokio::task::spawn_blocking(move || repo_scanner::prefetch_unique_repos(&dirs))
                .await
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
        tasks.spawn_blocking(move || {
            let _permit = permit;
            let update_available = refresh_single_skill_update(&path, &failed_roots);
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
    if let Ok(mut cache) = UPDATE_STATE_CACHE.write() {
        // Only update entries we got definitive results for; leave the rest
        // as-is so failed-fetch skills keep their previous state.
        for state in &states {
            cache.insert(state.name.clone(), state.update_available);
        }
    }
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

    paths.sort_by(|left, right| skill_name_from_path(left).cmp(&skill_name_from_path(right)));
    Ok(paths)
}

fn build_installed_skill(
    path: PathBuf,
    lock_entry: Option<LockEntry>,
    profiles: &[AgentProfile],
    target_language: Option<&str>,
    preloaded_translations: &HashMap<String, CachedTranslation>,
) -> Result<Skill> {
    // For repo-cached skills (symlinks into .repos/), resolve the actual path
    let is_repo_skill = repo_scanner::is_repo_cached_skill(&path);

    if !is_repo_skill {
        let _ = git_ops::ensure_worktree_checked_out(&path);
    }

    let name = skill_name_from_path(&path).unwrap_or_default();

    // For symlinked skills, read SKILL.md from the symlink target
    let effective_path = if is_repo_skill {
        let link_target = std::fs::read_link(&path);

        #[cfg(windows)]
        let link_target = link_target.or_else(|_| junction::get_target(&path));

        link_target
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
    let mut localized_description = None;

    // Fast HashMap lookup — zero DB cost (preloaded in bulk).
    if target_language.is_some() && !description.trim().is_empty() {
        let hex = crate::core::infra::util::sha256_hex(description.as_bytes());
        if let Some(cached) = preloaded_translations.get(&hex) {
            if !cached.translated_text.trim().is_empty() {
                localized_description = Some(cached.translated_text.clone());
            }
        }
    }

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
        crate::core::skill::SkillType::Local
    } else {
        crate::core::skill::SkillType::Hub
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
) -> Option<bool> {
    // For repo-cached skills, the repo has already been fetched by
    // prefetch_unique_repos; only compare local HEAD vs origin/HEAD.
    // Returns None when the prefetch failed for this skill's repo.
    if repo_scanner::is_repo_cached_skill(path) {
        return repo_scanner::check_repo_skill_update_local(path, failed_fetch_roots);
    }
    let _ = git_ops::ensure_worktree_checked_out(path);
    Some(git_ops::check_update(path).unwrap_or(false))
}

fn detect_agent_links(skill_name: &str, profiles: &[AgentProfile]) -> Vec<String> {
    let mut links = Vec::with_capacity(2); // most skills link to 1-2 agents
    for profile in profiles {
        let link_path = profile.global_skills_dir.join(skill_name);
        // Check symlinks/junctions: is_link() AND exists() (follows target — broken = false)
        if crate::core::infra::fs_ops::is_link(&link_path) && link_path.exists() {
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
