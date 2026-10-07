//! Read-only inspection of how a skill is deployed to a given agent directory.
//!
//! Deployment silently degrades to a directory copy when the OS cannot create
//! a symlink (e.g. Windows without Developer Mode). This exposes what actually
//! landed on disk, classified by [`super::ownership`], so the UI can badge it.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeployKind {
    /// No link/copy/directory present at the expected path.
    Missing,
    /// Symlink (Unix) or junction (Windows).
    Link,
    /// Full directory copy.
    Copy,
    /// An entry SkillStar does not own (e.g. the user made it manually).
    Unknown,
    /// The Agent reads the canonical root directly; nothing is deployed.
    CanonicalRoot,
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

/// Whether this platform permits link-based deployment without fallback.
pub fn developer_mode_available() -> bool {
    ss_core::infra::fs_ops::check_developer_mode()
}

/// Return the deploy status for `skill_name` under every enabled agent profile.
pub fn get_skill_deploy_status(skill_name: &str) -> Vec<AgentDeployStatus> {
    let profiles = crate::agents::list_profiles();
    let mut rows: Vec<AgentDeployStatus> = Vec::with_capacity(profiles.len());

    for profile in profiles {
        if !profile.enabled || !profile.has_global_skills() {
            continue;
        }
        let target = profile.global_skills_dir.join(skill_name);
        let target_str = target.to_string_lossy().to_string();

        let (kind, link_alive) = if super::targets_canonical_root(&profile.global_skills_dir) {
            (DeployKind::CanonicalRoot, target.exists())
        } else {
            match super::owned_deployment(&target, skill_name) {
                super::Ownership::Missing => (DeployKind::Missing, false),
                super::Ownership::Link { alive } => (DeployKind::Link, alive),
                super::Ownership::Copy => (DeployKind::Copy, true),
                super::Ownership::Foreign => (DeployKind::Unknown, true),
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

    rows
}
