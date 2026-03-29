//! Project-level skill configuration management.
//!
//! All config data lives in SkillStar's data directory (`skillstar/projects/`).
//! Project directories only receive symlinks — zero file pollution.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{agent_profile, sync};

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

/// Root data directory for SkillStar.
fn data_root() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local")
                .join("share")
        })
        .join("skillstar")
}

/// Path to the project index file.
fn index_path() -> PathBuf {
    data_root().join("projects.json")
}

/// Directory for a specific project's config files.
fn project_dir(name: &str) -> PathBuf {
    data_root().join("projects").join(name)
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

// ── Full Sync ───────────────────────────────────────────────────────

/// Perform a full sync: clear all existing symlinks in each agent's project
/// skill directory, then recreate them from the provided skills list.
///
/// Returns the total number of symlinks created.
pub fn full_sync(project_path: &str, skills_list: &SkillsList) -> Result<u32> {
    let hub_dir = sync::get_hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);
    let mut total = 0u32;

    for profile in &profiles {
        clear_project_symlinks(project, profile)?;
    }

    for (agent_id, skill_names) in &skills_list.agents {
        // Find the agent profile to get its project_skills_rel
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };

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
            if create_symlink(&source, &target).is_ok() {
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
    let hub_dir = sync::get_hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    let mut skills = Vec::with_capacity(profiles.len() * 8); // reasonable pre-alloc
    let mut agents_found = Vec::with_capacity(profiles.len());

    for profile in &profiles {
        let skills_dir = project.join(&profile.project_skills_rel);
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut found_any = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !path.is_symlink() {
                continue;
            }
            // For symlinks that point to directories, is_dir() returns true
            // but we also need is_symlink() check
            let is_symlink = path.is_symlink();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            let in_hub = hub_dir.join(&name).exists();
            let has_skill_md = path.join("SKILL.md").exists();

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

/// Import discovered skills into the hub and update the project's skills-list.
///
/// Strategy A: Copy + Replace with Symlink.
/// - If a skill doesn't exist in the hub, copy it there.
/// - Replace the original real directory in the project with a symlink.
/// - If the skill already exists in the hub, skip copying but still write
///   the mapping into skills-list.json.
/// - Finally, merge all imported skills into the project's skills-list.json.
pub fn import_scanned_skills(
    project_path: &str,
    project_name: &str,
    targets: &[ImportTarget],
) -> Result<ImportResult> {
    let entry = register_project(project_path)?;
    let canonical_project_name = entry.name;

    let hub_dir = sync::get_hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    std::fs::create_dir_all(&hub_dir)
        .with_context(|| format!("failed to create hub dir: {}", hub_dir.display()))?;

    let mut imported_to_hub = Vec::new();
    let mut symlink_count = 0u32;

    for target in targets {
        // Find the agent profile
        let Some(profile) = profiles.iter().find(|p| p.id == target.agent_id) else {
            continue;
        };

        let source_dir = project.join(&profile.project_skills_rel).join(&target.name);
        if !source_dir.exists() {
            continue;
        }

        // Skip if already a symlink (already managed)
        if source_dir.is_symlink() {
            continue;
        }

        let hub_skill_dir = hub_dir.join(&target.name);

        // Step 1: Copy to hub if not already there
        if !hub_skill_dir.exists() {
            copy_dir_recursive(&source_dir, &hub_skill_dir)
                .with_context(|| format!("failed to copy skill '{}' to hub", target.name))?;
            imported_to_hub.push(target.name.clone());
        }

        // Step 2: Replace real directory with symlink
        // Remove the real directory first, then create symlink
        std::fs::remove_dir_all(&source_dir)
            .with_context(|| format!("failed to remove real dir: {}", source_dir.display()))?;

        create_symlink(&hub_skill_dir, &source_dir)
            .with_context(|| format!("failed to create symlink for skill '{}'", target.name))?;

        symlink_count += 1;
    }

    // Step 3: Merge into skills-list.json.
    // Prefer the canonical registered project name. Fall back to the caller-provided
    // name to preserve pre-registration data from older flows.
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

    for target in targets {
        let agent_skills = existing.agents.entry(target.agent_id.clone()).or_default();
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

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .with_context(|| format!("failed to copy {:?}", src_path))?;
        }
    }
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Cross-platform symlink creation.
fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("failed to symlink {:?} -> {:?}", src, dst))?;

    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(src, dst)
        .with_context(|| format!("failed to symlink {:?} -> {:?}", src, dst))?;

    Ok(())
}

fn clear_project_symlinks(project: &Path, profile: &agent_profile::AgentProfile) -> Result<()> {
    let target_dir = project.join(&profile.project_skills_rel);
    let entries = match std::fs::read_dir(&target_dir) {
        Ok(entries) => Some(entries),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read skill dir: {}", target_dir.display()))
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
            if entry_path.is_symlink() {
                std::fs::remove_file(&entry_path).with_context(|| {
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
                    .with_context(|| format!("failed to prune empty dir: {}", current.display()))
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
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn full_sync_removes_symlinks_for_deselected_agents() -> Result<()> {
        let _guard = env_lock()
            .lock()
            .expect("environment lock should not be poisoned");

        let temp_root = make_temp_root("project-sync-remove")?;
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", temp_root.join("home"));

        let result = (|| -> Result<()> {
            let hub_skill = sync::get_hub_skills_dir().join("demo-skill");
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
            let codex_link = project_path.join(".agents/skills/demo-skill");
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
                "expected stale .agents symlink to be removed after deselecting agent"
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
                !project_path.join(".agents/skills").exists(),
                "expected empty .agents/skills directory to be pruned"
            );
            assert!(
                !project_path.join(".agents").exists(),
                "expected empty .agents directory to be pruned"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
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
        std::env::set_var("HOME", temp_root.join("home"));

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
                "expected legacy skill to be copied to hub during import"
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
            assert!(
                source_skill_dir.is_symlink(),
                "expected original project skill directory to be replaced with symlink"
            );

            Ok(())
        })();

        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
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
