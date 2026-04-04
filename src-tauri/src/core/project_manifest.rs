//! Project-level skill configuration management.
//!
//! All config data lives in SkillStar's data directory (`skillstar/projects/`).
//! Project directories only receive symlinks — zero file pollution.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{agent_profile, local_skill, paths};

// ── Data structures ─────────────────────────────────────────────────

/// An entry in the project index (`projects.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Absolute path to the project root.
    pub path: String,
    /// Project folder basename (used as the config folder name).
    pub name: String,
    /// ISO 8601 timestamp when the project was first registered.
    pub created_at: String,
}

/// The top-level index of all registered projects.
#[derive(Debug, Serialize, Deserialize, Default)]
struct ProjectIndex {
    projects: Vec<ProjectEntry>,
}

/// Per-project skill configuration (`projects/<name>/skills-list.json`).
/// Maps each agent ID to its own set of skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsList {
    /// `agent_id → [skill_name, ...]`
    pub agents: HashMap<String, Vec<String>>,
    /// ISO 8601 timestamp of last modification.
    pub updated_at: String,
}

// ── Paths ───────────────────────────────────────────────────────────

/// Path to the project index file.
fn index_path() -> PathBuf {
    paths::projects_manifest_path()
}

/// Directory for a specific project's config files.
fn project_dir(name: &str) -> PathBuf {
    paths::project_detail_dir(name)
}

/// Path to a project's skill list file.
fn skills_list_path(name: &str) -> PathBuf {
    project_dir(name).join("skills-list.json")
}

// ── Project Index CRUD ──────────────────────────────────────────────

fn load_index() -> ProjectIndex {
    let path = index_path();
    if !path.exists() {
        return ProjectIndex::default();
    }
    let Ok(content) = std::fs::read_to_string(&path) else {
        return ProjectIndex::default();
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_index(index: &ProjectIndex) -> Result<()> {
    let path = index_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content =
        serde_json::to_string_pretty(index).context("failed to serialize project index")?;
    std::fs::write(&path, content).context("failed to write project index")?;
    Ok(())
}

/// Register a project in the index. Creates the per-project config folder.
/// If a project with the same path already exists, returns the existing entry.
pub fn register_project(project_path: &str) -> Result<ProjectEntry> {
    let mut index = load_index();

    // Check for existing entry by path
    if let Some(existing) = index.projects.iter().find(|p| p.path == project_path) {
        return Ok(existing.clone());
    }

    let name = Path::new(project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Handle duplicate names by appending a suffix
    let unique_name = {
        let mut candidate = name.clone();
        let mut counter = 1u32;
        while index.projects.iter().any(|p| p.name == candidate) {
            counter += 1;
            candidate = format!("{name}-{counter}");
        }
        candidate
    };

    let entry = ProjectEntry {
        path: project_path.to_string(),
        name: unique_name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Create per-project config directory
    let dir = project_dir(&unique_name);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create project dir: {}", dir.display()))?;

    index.projects.push(entry.clone());
    save_index(&index)?;

    Ok(entry)
}

/// List all registered projects.
pub fn list_projects() -> Vec<ProjectEntry> {
    load_index().projects
}

/// Remove a project from the index and delete its config folder.
pub fn remove_project(name: &str) -> Result<()> {
    let mut index = load_index();
    let before = index.projects.len();
    index.projects.retain(|p| p.name != name);
    if index.projects.len() == before {
        anyhow::bail!("project '{}' not found", name);
    }
    save_index(&index)?;

    // Remove config folder
    let dir = project_dir(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to remove project dir: {}", dir.display()))?;
    }

    Ok(())
}

/// Update a project's local path and rebuild its symlinks.
pub fn update_project_path(name: &str, new_path: &str) -> Result<u32> {
    let mut index = load_index();
    let Some(entry) = index.projects.iter_mut().find(|p| p.name == name) else {
        anyhow::bail!("project '{}' not found", name);
    };

    entry.path = new_path.to_string();
    save_index(&index)?;

    // Rebuild mapped paths
    let skills_list = load_skills_list(name).unwrap_or(SkillsList {
        agents: HashMap::new(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    });

    let count = full_sync(new_path, &skills_list)?;
    Ok(count)
}

// ── Skills List CRUD ────────────────────────────────────────────────

/// Load a project's skill list by project name.
pub fn load_skills_list(name: &str) -> Option<SkillsList> {
    let path = skills_list_path(name);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save a project's skill list.
fn save_skills_list(name: &str, list: &SkillsList) -> Result<()> {
    let path = skills_list_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(list).context("failed to serialize skills list")?;
    std::fs::write(&path, content).context("failed to write skills list")?;
    Ok(())
}

/// Remove a skill from every registered project's persisted metadata and
/// project-level symlinks.
///
/// This is a targeted cleanup pass used when a skill is uninstalled from the
/// hub. It intentionally removes only the named symlink from project folders
/// instead of running a full project sync, so unmanaged on-disk directories are
/// left alone.
pub fn remove_skill_from_all_projects(skill_name: &str) -> Result<Vec<String>> {
    let profiles = agent_profile::list_profiles();
    let mut touched_projects = Vec::new();

    for entry in list_projects() {
        let mut touched = false;

        if let Some(mut skills_list) = load_skills_list(&entry.name) {
            let mut list_changed = false;
            for skill_names in skills_list.agents.values_mut() {
                let before = skill_names.len();
                skill_names.retain(|name| name != skill_name);
                if skill_names.len() != before {
                    list_changed = true;
                }
            }
            if list_changed {
                skills_list.agents.retain(|_, skills| !skills.is_empty());
                skills_list.updated_at = chrono::Utc::now().to_rfc3339();
                save_skills_list(&entry.name, &skills_list)?;
                touched = true;
            }
        }

        let project_root = Path::new(&entry.path);
        for profile in &profiles {
            if !profile.has_project_skills() {
                continue;
            }

            let skill_path = project_root
                .join(&profile.project_skills_rel)
                .join(skill_name);
            if !paths::is_link(&skill_path) {
                continue;
            }

            paths::remove_symlink(&skill_path).with_context(|| {
                format!(
                    "failed to remove project skill symlink '{}' from {}",
                    skill_name, entry.path
                )
            })?;

            if let Some(parent) = skill_path.parent() {
                prune_empty_dirs_upward(parent, project_root)?;
            }

            touched = true;
        }

        if touched {
            touched_projects.push(entry.name);
        }
    }

    Ok(touched_projects)
}

// ── Full Sync ───────────────────────────────────────────────────────

/// Perform a full sync: clear all existing symlinks in each agent's project
/// skill directory, then recreate them from the provided skills list.
///
/// Returns the total number of symlinks created.
pub fn full_sync(project_path: &str, skills_list: &SkillsList) -> Result<u32> {
    let hub_dir = paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);
    let mut total = 0u32;

    for profile in &profiles {
        if !profile.has_project_skills() {
            continue;
        }
        clear_project_symlinks(project, profile)?;
    }

    for (agent_id, skill_names) in &skills_list.agents {
        // Find the agent profile to get its project_skills_rel
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        // Skip agents that have no project-level skills support
        if !profile.has_project_skills() {
            continue;
        }

        let target_dir = project.join(&profile.project_skills_rel);
        std::fs::create_dir_all(&target_dir)
            .with_context(|| format!("failed to create skill dir: {}", target_dir.display()))?;

        // Create new symlinks
        for skill_name in skill_names {
            let source = hub_dir.join(skill_name);
            if !source.exists() {
                continue;
            }
            let target = target_dir.join(skill_name);
            if paths::create_symlink_or_copy(&source, &target).is_ok() {
                total += 1;
            }
        }
    }

    Ok(total)
}

/// Register a project, save its skills list, and perform a full sync.
///
/// This is the main entry point for both initial deployment and subsequent
/// modifications. Returns `(project_name, symlink_count)`.
pub fn save_and_sync(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
) -> Result<(String, u32)> {
    let entry = register_project(project_path)?;

    let skills_list = SkillsList {
        agents,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    save_skills_list(&entry.name, &skills_list)?;
    let count = full_sync(project_path, &skills_list)?;

    Ok((entry.name, count))
}

/// Register a project and persist its skills-list.json without mutating any
/// project filesystem symlinks.
///
/// This is used for non-destructive metadata updates (for example, resolving
/// shared-path ownership) where we must not clear or recreate links.
pub fn save_skills_list_only(
    project_path: &str,
    agents: HashMap<String, Vec<String>>,
) -> Result<SkillsList> {
    let entry = register_project(project_path)?;

    let skills_list = SkillsList {
        agents,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    save_skills_list(&entry.name, &skills_list)?;

    Ok(skills_list)
}

/// Rebuild a project's skills-list.json from on-disk project skill directories.
///
/// For shared paths (multiple agents with the same `project_skills_rel`), this
/// function picks a single owner agent:
/// 1) prefer an agent that already exists in current skills-list.json
/// 2) otherwise use the first agent in builtin profile order.
///
/// It persists and returns the rebuilt list, without performing full sync.
pub fn rebuild_skills_list_from_disk(project_path: &str) -> Result<SkillsList> {
    let entry = register_project(project_path)?;
    let project = Path::new(project_path);
    let profiles = agent_profile::list_profiles();
    let existing_agents = load_skills_list(&entry.name)
        .map(|list| list.agents)
        .unwrap_or_default();

    // Group profiles by project_skills_rel while preserving profile order.
    let mut path_order = Vec::new();
    let mut groups: HashMap<String, Vec<agent_profile::AgentProfile>> = HashMap::new();
    for profile in profiles {
        if !profile.has_project_skills() {
            continue;
        }
        if !groups.contains_key(&profile.project_skills_rel) {
            path_order.push(profile.project_skills_rel.clone());
        }
        groups
            .entry(profile.project_skills_rel.clone())
            .or_default()
            .push(profile);
    }

    let mut rebuilt_agents: HashMap<String, Vec<String>> = HashMap::new();

    for rel_path in path_order {
        let Some(group_profiles) = groups.get(&rel_path) else {
            continue;
        };
        let skills_dir = project.join(&rel_path);
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut names = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !paths::is_link(&path) {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            // Keep explicit skill folders and symlinked skills.
            // Skip arbitrary non-skill directories without SKILL.md.
            let has_skill_md = path.join("SKILL.md").exists();
            if !paths::is_link(&path) && !has_skill_md {
                continue;
            }

            if !names.contains(&name) {
                names.push(name);
            }
        }

        if names.is_empty() {
            continue;
        }

        // Prefer previously configured owner for shared paths.
        let owner = group_profiles
            .iter()
            .find(|profile| existing_agents.contains_key(&profile.id))
            .or_else(|| group_profiles.first())
            .map(|profile| profile.id.clone());

        let Some(owner_id) = owner else {
            continue;
        };

        let bucket = rebuilt_agents.entry(owner_id).or_default();
        bucket.extend(names);
    }

    for names in rebuilt_agents.values_mut() {
        names.sort();
        names.dedup();
    }

    let rebuilt = SkillsList {
        agents: rebuilt_agents,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    save_skills_list(&entry.name, &rebuilt)?;

    Ok(rebuilt)
}

// ── Agent Detection ─────────────────────────────────────────────────

/// A single agent's detection result for a project directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedAgent {
    pub agent_id: String,
    pub display_name: String,
    pub icon: String,
    pub project_skills_rel: String,
    pub exists: bool,
}

/// A group of agents that share the same project-level skill directory,
/// where that directory actually exists in the project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AmbiguousGroup {
    pub path: String,
    pub agent_ids: Vec<String>,
    pub agent_names: Vec<String>,
}

/// Result of detecting which agents are present in a project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectAgentDetection {
    /// Per-agent detection results.
    pub detected: Vec<DetectedAgent>,
    /// Groups of agents that share the same path AND that path exists.
    /// The frontend should prompt the user to choose which agent(s) to enable.
    pub ambiguous_groups: Vec<AmbiguousGroup>,
    /// Agent IDs that have a unique path and that path exists — safe to auto-enable.
    pub auto_enable: Vec<String>,
}

/// Scan a project directory for existing agent skill directories.
///
/// For each agent profile, check if `<project_root>/<project_skills_rel>` exists.
/// Unique paths that exist → auto-enable. Shared paths that exist → ambiguous group.
pub fn detect_project_agents(project_path: &str) -> ProjectAgentDetection {
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    // Build detection list
    let detected: Vec<DetectedAgent> = profiles
        .iter()
        .filter(|p| p.has_project_skills())
        .map(|p| {
            let skills_dir = project.join(&p.project_skills_rel);
            DetectedAgent {
                agent_id: p.id.clone(),
                display_name: p.display_name.clone(),
                icon: p.icon.clone(),
                project_skills_rel: p.project_skills_rel.clone(),
                exists: skills_dir.exists(),
            }
        })
        .collect();

    // Group agents by project_skills_rel
    let mut path_groups: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for d in &detected {
        if d.exists {
            path_groups
                .entry(d.project_skills_rel.clone())
                .or_default()
                .push((d.agent_id.clone(), d.display_name.clone()));
        }
    }

    let mut ambiguous_groups = Vec::new();
    let mut auto_enable = Vec::new();

    for (path, agents) in &path_groups {
        if agents.len() > 1 {
            ambiguous_groups.push(AmbiguousGroup {
                path: path.clone(),
                agent_ids: agents.iter().map(|(id, _)| id.clone()).collect(),
                agent_names: agents.iter().map(|(_, name)| name.clone()).collect(),
            });
        } else if let Some((id, _)) = agents.first() {
            auto_enable.push(id.clone());
        }
    }

    // Disambiguation sealed — each agent now has a unique project_skills_rel,
    // so ambiguous groups can no longer occur. The detection logic above is
    // preserved but its output is discarded.
    ProjectAgentDetection {
        detected,
        ambiguous_groups: Vec::new(),
        auto_enable,
    }
}

// ── Scan & Import ───────────────────────────────────────────────────

/// A single skill entry discovered by scanning a project directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScannedSkill {
    pub name: String,
    pub agent_id: String,
    pub is_symlink: bool,
    pub in_hub: bool,
    pub has_skill_md: bool,
}

/// Result of scanning a project for existing skill directories.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectScanResult {
    pub skills: Vec<ScannedSkill>,
    pub agents_found: Vec<String>,
}

/// What the caller wants to import.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportTarget {
    pub name: String,
    pub agent_id: String,
}

/// Result of an import operation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportResult {
    pub imported_to_hub: Vec<String>,
    pub skills_list_updated: bool,
    pub symlink_count: u32,
}

/// Scan a project directory for existing skill entries across all agent profiles.
///
/// For each agent profile, look at `<project>/<project_skills_rel>/` and inspect
/// every child directory. Classify each as symlink vs real directory, check
/// whether the hub already has a skill with the same name, and whether the
/// directory contains a SKILL.md file.
pub fn scan_project_skills(project_path: &str) -> ProjectScanResult {
    let hub_dir = paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    let mut skills = Vec::with_capacity(profiles.len() * 8); // reasonable pre-alloc
    let mut agents_found = Vec::with_capacity(profiles.len());

    for profile in &profiles {
        if !profile.has_project_skills() {
            continue;
        }
        let skills_dir = project.join(&profile.project_skills_rel);
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut found_any = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !paths::is_link(&path) {
                continue;
            }
            // For symlinks that point to directories, is_dir() returns true
            // but we also need is_symlink() check
            let is_symlink = paths::is_link(&path);
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            let in_hub = hub_dir.join(&name).exists();
            let has_skill_md = path.join("SKILL.md").exists();
            if !is_symlink && !has_skill_md {
                continue;
            }

            skills.push(ScannedSkill {
                name,
                agent_id: profile.id.clone(),
                is_symlink,
                in_hub,
                has_skill_md,
            });
            found_any = true;
        }

        if found_any {
            agents_found.push(profile.id.clone());
        }
    }

    ProjectScanResult {
        skills,
        agents_found,
    }
}

/// Import discovered skills into local storage and update the project's
/// skills-list.
///
/// Strategy A: Adopt + Replace with Symlink.
/// - If a skill doesn't exist in the hub, move it into `skills-local/` and
///   expose it through the hub symlink in `skills/`.
/// - Replace the original real directory in the project with a symlink to the
///   hub entry.
/// - If the skill already exists in the hub, skip adoption but still write
///   the mapping into `skills-list.json`.
/// - Finally, merge all imported skills into the project's skills-list.json.
pub fn import_scanned_skills(
    project_path: &str,
    project_name: &str,
    targets: &[ImportTarget],
) -> Result<ImportResult> {
    let entry = register_project(project_path)?;
    let canonical_project_name = entry.name;

    let hub_dir = paths::hub_skills_dir();
    local_skill::reconcile_hub_symlinks();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    std::fs::create_dir_all(&hub_dir)
        .with_context(|| format!("failed to create hub dir: {}", hub_dir.display()))?;

    // Preserve any previously chosen owner for shared project paths.
    let mut existing = load_skills_list(&canonical_project_name)
        .or_else(|| {
            if project_name != canonical_project_name.as_str() {
                load_skills_list(project_name)
            } else {
                None
            }
        })
        .unwrap_or(SkillsList {
            agents: HashMap::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });

    let mut owner_by_path: HashMap<String, String> = HashMap::new();
    for agent_id in existing.agents.keys() {
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        if !profile.has_project_skills() {
            continue;
        }
        owner_by_path
            .entry(profile.project_skills_rel.clone())
            .or_insert_with(|| agent_id.clone());
    }

    let mut imported_to_hub = Vec::new();
    let mut symlink_count = 0u32;

    for target in targets {
        // Find the agent profile used to locate the on-disk project folder.
        let Some(source_profile) = profiles.iter().find(|p| p.id == target.agent_id) else {
            continue;
        };
        if !source_profile.has_project_skills() {
            continue;
        }

        let source_dir = project
            .join(&source_profile.project_skills_rel)
            .join(&target.name);
        if !source_dir.exists() {
            continue;
        }

        // Skip if already a symlink (already managed)
        if paths::is_link(&source_dir) {
            continue;
        }

        // Only valid skill folders are safe to import and replace.
        if !source_dir.join("SKILL.md").exists() {
            continue;
        }

        let effective_agent_id = owner_by_path
            .get(&source_profile.project_skills_rel)
            .cloned()
            .unwrap_or_else(|| target.agent_id.clone());
        owner_by_path
            .entry(source_profile.project_skills_rel.clone())
            .or_insert_with(|| effective_agent_id.clone());

        let hub_skill_dir = hub_dir.join(&target.name);

        // Step 1: Adopt into local storage if not already present in the hub.
        if !hub_skill_dir.exists() {
            local_skill::adopt_existing_dir(&target.name, &source_dir).with_context(|| {
                format!(
                    "failed to adopt discovered project skill '{}' into skills-local",
                    target.name
                )
            })?;
            imported_to_hub.push(target.name.clone());
        } else {
            // Step 2a: Skill already exists in the hub, so replace the
            // unmanaged project copy with a symlink to the canonical hub entry.
            std::fs::remove_dir_all(&source_dir)
                .with_context(|| format!("failed to remove real dir: {}", source_dir.display()))?;
        }

        // Step 2b: Point the project entry at the hub entry, which may itself
        // be a symlink into `skills-local/`.
        paths::create_symlink_or_copy(&hub_skill_dir, &source_dir)
            .with_context(|| format!("failed to create symlink for skill '{}'", target.name))?;

        symlink_count += 1;

        let agent_skills = existing.agents.entry(effective_agent_id).or_default();
        if !agent_skills.contains(&target.name) {
            agent_skills.push(target.name.clone());
        }
    }

    existing.updated_at = chrono::Utc::now().to_rfc3339();
    save_skills_list(&canonical_project_name, &existing)?;

    Ok(ImportResult {
        imported_to_hub,
        skills_list_updated: true,
        symlink_count,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────

fn clear_project_symlinks(project: &Path, profile: &agent_profile::AgentProfile) -> Result<()> {
    if !profile.has_project_skills() {
        return Ok(());
    }
    let target_dir = project.join(&profile.project_skills_rel);
    let entries = match std::fs::read_dir(&target_dir) {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read skill dir: {}", target_dir.display()));
        }
    };

    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.with_context(|| {
                format!(
                    "failed to inspect project skill entry in {}",
                    target_dir.display()
                )
            })?;
            let entry_path = entry.path();
            if paths::is_link(&entry_path) {
                paths::remove_symlink(&entry_path).with_context(|| {
                    format!("failed to remove stale symlink: {}", entry_path.display())
                })?;
            }
        }
    }

    prune_empty_dirs_upward(&target_dir, project)?;

    Ok(())
}

fn prune_empty_dirs_upward(start_dir: &Path, project_root: &Path) -> Result<()> {
    let mut current = start_dir.to_path_buf();

    while current.starts_with(project_root) && current != project_root {
        match std::fs::remove_dir(&current) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to prune empty dir: {}", current.display()));
            }
        }

        let Some(parent) = current.parent() else {
            break;
        };
        current = parent.to_path_buf();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::ffi::OsStr;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static std::sync::Mutex<()> {
        crate::core::test_env_lock()
    }

    fn set_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(key: K, value: V) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env<K: AsRef<OsStr>>(key: K) {
        unsafe { std::env::remove_var(key) }
    }

    #[test]
    fn full_sync_removes_symlinks_for_deselected_agents() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-sync-remove")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;

            let mut first_agents = HashMap::new();
            first_agents.insert("claude".to_string(), vec!["demo-skill".to_string()]);
            first_agents.insert("codex".to_string(), vec!["demo-skill".to_string()]);
            let first = SkillsList {
                agents: first_agents,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            full_sync(&project_path.to_string_lossy(), &first)?;

            let claude_link = project_path.join(".claude/skills/demo-skill");
            let codex_link = project_path.join(".codex/skills/demo-skill");
            assert!(
                claude_link.is_symlink(),
                "expected initial symlink to exist"
            );
            assert!(
                codex_link.is_symlink(),
                "expected initial codex symlink to exist"
            );

            let second = SkillsList {
                agents: HashMap::new(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            full_sync(&project_path.to_string_lossy(), &second)?;

            assert!(
                !claude_link.symlink_metadata().is_ok(),
                "expected stale symlink to be removed after deselecting agent"
            );
            assert!(
                !codex_link.symlink_metadata().is_ok(),
                "expected stale .codex symlink to be removed after deselecting agent"
            );
            assert!(
                !project_path.join(".claude/skills").exists(),
                "expected empty .claude/skills directory to be pruned"
            );
            assert!(
                !project_path.join(".claude").exists(),
                "expected empty .claude directory to be pruned"
            );
            assert!(
                !project_path.join(".codex/skills").exists(),
                "expected empty .codex/skills directory to be pruned"
            );
            assert!(
                !project_path.join(".codex").exists(),
                "expected empty .codex directory to be pruned"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn import_scanned_skills_registers_project_when_missing() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-import-register")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            let source_skill_dir = project_path.join(".claude/skills/legacy-skill");

            std::fs::create_dir_all(&source_skill_dir)?;
            std::fs::write(source_skill_dir.join("SKILL.md"), "description: legacy")?;

            let targets = vec![ImportTarget {
                name: "legacy-skill".to_string(),
                agent_id: "claude".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert!(import_result.skills_list_updated);
            assert!(
                import_result
                    .imported_to_hub
                    .iter()
                    .any(|name| name == "legacy-skill"),
                "expected legacy skill to be exposed through the hub during import"
            );

            let registered = list_projects()
                .into_iter()
                .find(|project| project.path == project_path_str)
                .expect("expected imported project to be auto-registered");
            let skills_list = load_skills_list(&registered.name)
                .expect("expected skills-list.json for registered project");
            let claude_skills = skills_list
                .agents
                .get("claude")
                .expect("expected imported skills under claude agent");
            assert!(
                claude_skills.iter().any(|skill| skill == "legacy-skill"),
                "expected imported skill to be present in project's skills list"
            );
            let local_skill_dir = crate::core::paths::local_skills_dir().join("legacy-skill");
            let hub_skill_dir = paths::hub_skills_dir().join("legacy-skill");
            assert!(
                local_skill_dir.is_dir(),
                "expected imported skill to be moved into skills-local"
            );
            assert!(
                hub_skill_dir.is_symlink(),
                "expected hub entry for imported skill to be a symlink"
            );
            assert_eq!(
                std::fs::read_link(&hub_skill_dir)?,
                local_skill_dir,
                "expected hub entry to point at skills-local storage"
            );
            assert!(
                source_skill_dir.is_symlink(),
                "expected original project skill directory to be replaced with symlink"
            );
            assert_eq!(
                std::fs::read_link(&source_skill_dir)?,
                hub_skill_dir,
                "expected project skill directory to point at the canonical hub entry"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn import_scanned_skills_skips_non_skill_directories() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-import-skip-invalid")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            let source_dir = project_path.join(".claude/skills/not-a-skill");

            std::fs::create_dir_all(&source_dir)?;
            std::fs::write(source_dir.join("README.md"), "not a skill")?;

            let targets = vec![ImportTarget {
                name: "not-a-skill".to_string(),
                agent_id: "claude".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;

            assert!(
                import_result.imported_to_hub.is_empty(),
                "expected invalid directories to be skipped during import"
            );
            assert_eq!(
                import_result.symlink_count, 0,
                "expected invalid directories to remain untouched"
            );
            assert!(
                source_dir.is_dir() && !source_dir.is_symlink(),
                "expected invalid source directory to remain a real directory"
            );

            let registered = list_projects()
                .into_iter()
                .find(|project| project.path == project_path_str)
                .expect("expected project registration during import");
            let skills_list = load_skills_list(&registered.name)
                .expect("expected skills-list.json for registered project");
            assert!(
                !skills_list
                    .agents
                    .values()
                    .flatten()
                    .any(|name| name == "not-a-skill"),
                "expected invalid directories to be excluded from project metadata"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn import_scanned_skills_preserves_shared_path_owner() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-import-owner")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let project_path = temp_root.join("workspace").join("demo-import-project");
            let project_path_str = project_path.to_string_lossy().to_string();
            // With Codex now using .codex/skills (unique path), the import
            // should attribute the skill to Codex directly, not merge it into
            // antigravity's .agents/skills path.
            let source_skill_dir = project_path.join(".codex/skills/shared-skill");

            std::fs::create_dir_all(&source_skill_dir)?;
            std::fs::write(source_skill_dir.join("SKILL.md"), "description: shared")?;

            let entry = register_project(&project_path_str)?;
            let existing = SkillsList {
                agents: HashMap::from([("antigravity".to_string(), Vec::new())]),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            save_skills_list(&entry.name, &existing)?;

            let targets = vec![ImportTarget {
                name: "shared-skill".to_string(),
                agent_id: "codex".to_string(),
            }];

            let import_result =
                import_scanned_skills(&project_path_str, "demo-import-project", &targets)?;
            assert_eq!(import_result.symlink_count, 1);

            let skills_list = load_skills_list(&entry.name)
                .expect("expected updated skills-list.json for registered project");
            // Codex now has its own unique path (.codex/skills), so the import
            // should attribute the skill to codex, not antigravity.
            assert!(
                skills_list
                    .agents
                    .get("codex")
                    .is_some_and(|skills| skills.iter().any(|skill| skill == "shared-skill")),
                "expected codex to own the imported skill at its unique .codex/skills path"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    #[test]
    fn remove_skill_from_all_projects_cleans_metadata_and_symlinks() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-remove-skill")?;
        let previous_home = std::env::var_os("HOME");
        set_env("HOME", temp_root.join("home"));
        #[cfg(windows)]
        let previous_userprofile = std::env::var_os("USERPROFILE");
        #[cfg(windows)]
        set_env("USERPROFILE", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = paths::hub_skills_dir().join("demo-skill");
            std::fs::create_dir_all(&hub_skill)?;
            std::fs::write(hub_skill.join("SKILL.md"), "description: test")?;

            let project_path = temp_root.join("workspace").join("demo-project");
            std::fs::create_dir_all(&project_path)?;

            let mut agents = HashMap::new();
            agents.insert("claude".to_string(), vec!["demo-skill".to_string()]);
            save_and_sync(&project_path.to_string_lossy(), agents)?;

            let project = list_projects()
                .into_iter()
                .find(|project| project.path == project_path.to_string_lossy())
                .expect("expected project to be registered");
            let link_path = project_path.join(".claude/skills/demo-skill");
            assert!(
                link_path.is_symlink(),
                "expected project skill symlink before cleanup"
            );

            let touched = remove_skill_from_all_projects("demo-skill")?;
            assert!(
                touched.iter().any(|name| name == &project.name),
                "expected cleanup to report the touched project"
            );

            let skills_list = load_skills_list(&project.name)
                .expect("expected project skills list to remain readable");
            assert!(
                skills_list.agents.is_empty(),
                "expected removed skill to be pruned from project metadata"
            );
            assert!(
                !link_path.symlink_metadata().is_ok(),
                "expected project skill symlink to be removed"
            );
            assert!(
                !project_path.join(".claude/skills").exists(),
                "expected empty project skill directory to be pruned"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => set_env("HOME", value),
            None => remove_env("HOME"),
        }
        #[cfg(windows)]
        match previous_userprofile {
            Some(value) => set_env("USERPROFILE", value),
            None => remove_env("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&temp_root);

        result
    }

    fn make_temp_root(suffix: &str) -> Result<PathBuf> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("failed to read system time")?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "skillstar-project-manifest-{}-{}-{}",
            suffix,
            std::process::id(),
            stamp
        ));
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create temp dir: {}", dir.display()))?;
        Ok(dir)
    }
}
