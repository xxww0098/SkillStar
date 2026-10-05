//! Cross-domain "install then deploy" use case for GUI installs.
//!
//! The CLI install flow deploys freshly installed hub skills to the selected
//! Agents as part of its command (`cli::install`). GUI installs
//! (`install_skill`, `install_from_scan`) only wrote to the hub, leaving
//! enabled Agents silently out of sync. This module gives both GUI entry
//! points the same post-install deploy step: link the installed skills into
//! every Agent the user has enabled in Settings.
//!
//! Semantics match the CLI:
//! - Skills already linked for an Agent are skipped (idempotent).
//! - No enabled global Agent: deploy is a no-op — the hub install itself
//!   remains valid and the user can enable Agents later.
//! - Partial deploy failure fails the call with a per-Agent summary, so the
//!   UI can surface "installed to the hub, but deployment is incomplete"
//!   instead of pretending everything synced.

/// Link `skill_names` into the given Agents only (carousel / `--agent`).
pub fn deploy_to_selected_global_agents(
    skill_names: &[String],
    agent_ids: &[String],
) -> Result<Vec<String>, String> {
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    skillstar_skills::deployment::batch_deploy_skills_to_agents(
        skill_names,
        agent_ids,
        skillstar_skills::projects::ProjectDeployMode::Symlink,
    )
    .map(|_| agent_ids.to_vec())
    .map_err(|err| format!("Installed to the hub but deployment is incomplete: {err:#}"))
}

