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
    /// Symlink (Unix) or symlink / junction (Windows via [`ss_core::infra::fs_ops::create_symlink`]).
    #[default]
    Symlink,
    /// Full directory copy; no live link to the hub.
    Copy,
}

/// Deploy a skill from hub into a project directory using an explicit mode.
///
/// - [`ProjectDeployMode::Symlink`] tries symlink/junction first and
///   auto-falls back to a full directory copy when the OS refuses (e.g.
///   Windows without Developer Mode on a cross-drive path).
/// - [`ProjectDeployMode::Copy`] always performs a full directory copy with
///   no live link to the hub.
///
/// There is deliberately no mode-less "auto" variant: a caller that already
/// knows it is refreshing a copy must not fall back into symlink-first, or it
/// downgrades a deployment the user explicitly chose.
pub fn deploy_skill_with_mode(source: &Path, target: &Path, mode: ProjectDeployMode) -> Result<()> {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::content::validate_skill_name(&name)?;
    match mode {
        ProjectDeployMode::Symlink => {
            let was_copy =
                crate::deployment::ownership::deploy_link_or_copy(source, target, &name)?;
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
        ProjectDeployMode::Copy => crate::deployment::ownership::deploy_copy(source, target, &name),
    }
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

#[derive(Debug, Clone, Default)]
pub struct CascadeUpdateSummary {
    pub projects_updated: Vec<String>,
    pub failures: Vec<String>,
}

/// Current facts about one persisted project deployment. The project manifest
/// supplies membership; copy ownership still requires its on-disk marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCopyState {
    Current,
    Stale,
    ProjectMissing,
    Missing,
    Modified,
    Unmanaged,
    SourceMissing,
    ReadFailed,
    BrokenLink,
}

/// Configuration and deployment facts observed under the same project lock.
#[derive(Debug, Clone)]
pub struct ProjectCopySnapshot {
    pub project: ProjectEntry,
    pub skills_list: SkillsList,
    pub copies: Vec<ProjectCopyRecord>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectCopyRecord {
    pub(super) project_name: String,
    pub(super) project_path: String,
    pub(super) relative_path: String,
    pub(super) skill_name: String,
    pub(super) state: ProjectCopyState,
    pub(super) detail: Option<String>,
}

impl ProjectCopyRecord {
    pub fn project_name(&self) -> &str {
        &self.project_name
    }
    pub fn project_path(&self) -> &str {
        &self.project_path
    }
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    pub fn skill_name(&self) -> &str {
        &self.skill_name
    }
    pub fn state(&self) -> ProjectCopyState {
        self.state
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}
