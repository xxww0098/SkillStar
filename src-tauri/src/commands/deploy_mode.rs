//! Read-only inspection of how a skill is deployed to a given agent directory.
//!
//! Today `create_symlink_or_copy` silently degrades to a directory copy when
//! the OS cannot create a symlink (e.g. Windows without Developer Mode). The
//! UI previously had no way to tell what actually landed on disk. This
//! command exposes the deployed-kind so the frontend can surface a badge.

use serde::Serialize;

use skillstar_core::infra::error::AppError;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployKind {
    /// No link/copy/directory present at the expected path.
    Missing,
    /// Symlink (Unix) or junction (Windows).
    Link,
    /// Full directory copy.
    Copy,
    /// A regular directory that is neither a link nor one of our copies
    /// (e.g. the user made it manually or a previous version).
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentDeployStatus {
    pub agent_id: String,
    pub agent_name: String,
    pub target_path: String,
    pub kind: DeployKind,
    /// `true` when `kind == Link` and the link resolves to a live directory.
    pub link_alive: bool,
}

/// Return the deploy status for `skill_name` under every enabled agent profile.
#[tauri::command]
pub async fn get_skill_deploy_status(
    skill_name: String,
) -> Result<Vec<AgentDeployStatus>, AppError> {
    tokio::task::spawn_blocking(move || compute_status(&skill_name))
        .await
        .map_err(|e| AppError::Other(format!("deploy-status task panicked: {e}")))?
}

fn compute_status(skill_name: &str) -> Result<Vec<AgentDeployStatus>, AppError> {
    let profiles = skillstar_projects::projects::agents::list_profiles();
    let mut rows: Vec<AgentDeployStatus> = Vec::with_capacity(profiles.len());

    for profile in profiles {
        if !profile.enabled {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        let target_str = target.to_string_lossy().to_string();

        let meta = target.symlink_metadata().ok();
        let (kind, link_alive) = match meta {
            None => (DeployKind::Missing, false),
            Some(_) => {
                if skillstar_core::infra::fs_ops::is_link(&target) {
                    let alive = target.exists();
                    (DeployKind::Link, alive)
                } else if target.is_dir() {
                    // Differentiate "our copy" from "user dir": our copy will contain
                    // SKILL.md. If so, treat as Copy; otherwise Unknown.
                    if target.join("SKILL.md").is_file() {
                        (DeployKind::Copy, true)
                    } else {
                        (DeployKind::Unknown, true)
                    }
                } else {
                    (DeployKind::Unknown, true)
                }
            }
        };

        rows.push(AgentDeployStatus {
            agent_id: profile.id,
            agent_name: profile.display_name,
            target_path: target_str,
            kind,
            link_alive,
        });
    }

    Ok(rows)
}
