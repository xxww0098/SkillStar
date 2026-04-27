//! Project manifest enums and serializable structures.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::agents as agent_profile;

/// How skills are deployed into a project-level agent directory (`project_skills_rel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDeployMode {
    /// Symlink (Unix) or symlink / junction (Windows via [`skillstar_infra::fs_ops::create_symlink`]).
    #[default]
    Symlink,
    /// Full directory copy; no live link to the hub.
    Copy,
}

/// Deploy a skill from hub into a project directory.
///
/// Always tries symlink first; if symlink creation fails (e.g. on Windows
/// without Developer Mode on a cross-drive path), falls back to a full
/// directory copy automatically. The caller no longer needs to specify a mode.
pub fn deploy_skill_auto(source: &Path, target: &Path) -> Result<()> {
    let was_copy = skillstar_infra::fs_ops::create_symlink_or_copy(source, target)?;
    if was_copy {
        tracing::info!(
            target: "sync",
            source = %source.display(),
            target = %target.display(),
            "Symlink failed, deployed via copy fallback",
        );
    }
    Ok(())
}

/// Drop deploy-mode entries for paths that no longer have an enabled agent in `agents`.
pub fn prune_deploy_modes_for_agents(
    deploy_modes: &mut HashMap<String, ProjectDeployMode>,
    agents: &HashMap<String, Vec<String>>,
    profiles: &[agent_profile::AgentProfile],
) {
    let mut keep = HashSet::new();
    for agent_id in agents.keys() {
        let Some(profile) = profiles.iter().find(|p| &p.id == agent_id) else {
            continue;
        };
        if profile.has_project_skills() {
            keep.insert(profile.project_skills_rel.clone());
        }
    }
    deploy_modes.retain(|path, _| keep.contains(path));
}

pub fn ensure_project_root_exists(project_path: &str) -> Result<PathBuf> {
    let project = Path::new(project_path);
    if project.is_dir() {
        return Ok(project.to_path_buf());
    }

    anyhow::bail!(
        "Project path not found or not a directory: {}\n\
请在「Projects」页面点击「更改路径」重新关联该项目目录。",
        project.display()
    )
}

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
pub struct ProjectIndex {
    pub projects: Vec<ProjectEntry>,
}

/// Per-project skill configuration (`projects/<name>/skills-list.json`).
/// Maps each agent ID to its own set of skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsList {
    /// `agent_id → [skill_name, ...]`
    pub agents: HashMap<String, Vec<String>>,
    /// `project_skills_rel` path (e.g. `.agents/skills`) → deploy mode for that directory.
    #[serde(default)]
    pub deploy_modes: HashMap<String, ProjectDeployMode>,
    /// ISO 8601 timestamp of last modification.
    pub updated_at: String,
}

impl Default for SkillsList {
    fn default() -> Self {
        Self {
            agents: HashMap::new(),
            deploy_modes: HashMap::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

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

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct CascadeUpdateSummary {
    pub projects_updated: Vec<String>,
}
