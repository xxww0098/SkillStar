//! Explicit link of hub Skills into chosen Agent directories.
//!
//! Installing a Skill writes the canonical copy only. A new Skill is not
//! linked into every enabled Agent. Linking is a separate step the user
//! asks for: a carousel click, the batch "link to agent" menu, or a deck's
//! deploy-all. This module is the selected-Agent step.
//!
//! - Skills already linked for an Agent are skipped (idempotent).
//! - An empty target list is a no-op — the hub install stays valid.
//! - Partial deploy failure fails the call with a per-Agent summary, so the
//!   UI can surface "installed to the hub, but deployment is incomplete"
//!   instead of pretending the requested Agents synced.

/// Link `skill_names` into the given Agents only (carousel / `--agent`).
pub fn deploy_to_selected_global_agents(
    skill_names: &[String],
    agent_ids: &[String],
) -> Result<Vec<String>, String> {
    if agent_ids.is_empty() {
        return Ok(Vec::new());
    }
    crate::deployment::batch_deploy_skills_to_agents(
        skill_names,
        agent_ids,
        crate::projects::ProjectDeployMode::Symlink,
    )
    .map(|_| agent_ids.to_vec())
    .map_err(|err| format!("Installed to the hub but deployment is incomplete: {err:#}"))
}
