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

/// Link `skill_names` into every enabled global-skill Agent profile.
///
/// Returns the ids of the Agents the skills were (already or newly) linked
/// to. Errors when at least one enabled Agent could not be deployed.
pub fn deploy_to_enabled_global_agents(skill_names: &[String]) -> Result<Vec<String>, String> {
    let enabled_ids = skillstar_agents::list_profiles()
        .iter()
        .filter(|profile| profile.enabled && profile.has_global_skills())
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();

    // No enabled global Agent is a no-op, not an error: the hub install stands
    // on its own. `batch_deploy_skills_to_agents` rejects an empty target list,
    // so that case has to be answered here.
    if enabled_ids.is_empty() {
        return Ok(enabled_ids);
    }

    // Filtering on `enabled` above is what the per-Agent entry point would
    // re-check, so this reuses the CLI deploy instead of looping: it is the
    // path that deduplicates targets by resolved directory, which matters for
    // the Agents that share one (`~/.agents/skills`).
    skillstar_skills::deployment::batch_deploy_skills_to_agents(
        skill_names,
        &enabled_ids,
        skillstar_skills::projects::ProjectDeployMode::Symlink,
    )
    .map(|_| enabled_ids)
    .map_err(|err| format!("Installed to the hub but deployment is incomplete: {err:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_LOCK, EnvGuard};

    /// With nothing enabled the hub install still stands on its own, so this
    /// stays a no-op instead of surfacing the "No target agents selected"
    /// error the underlying batch deploy returns for an empty target list.
    #[tokio::test]
    async fn no_enabled_agent_is_a_no_op() {
        let _lock = ENV_LOCK.lock().await;
        let temp = tempfile::tempdir().unwrap();
        let _env = EnvGuard::set(&[
            ("SKILLSTAR_DATA_DIR", &temp.path().join("data")),
            ("SKILLSTAR_HUB_DIR", &temp.path().join("hub")),
        ]);

        assert_eq!(
            deploy_to_enabled_global_agents(&["demo-skill".to_string()]).unwrap(),
            Vec::<String>::new()
        );
    }
}
