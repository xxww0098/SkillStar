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
/// Link `skill_names` into every enabled global-skill Agent profile.
///
/// Returns the ids of the Agents the skills were (already or newly) linked
/// to. Errors when at least one enabled Agent could not be deployed.
pub fn deploy_to_enabled_global_agents(skill_names: &[String]) -> Result<Vec<String>, String> {
    let profiles = skillstar_agents::list_profiles();
    let enabled_ids = profiles
        .iter()
        .filter(|profile| profile.enabled && profile.has_global_skills())
        .map(|profile| profile.id.clone())
        .collect::<Vec<_>>();

    let mut deployed_to = Vec::with_capacity(enabled_ids.len());
    let mut failures = Vec::new();
    for agent_id in enabled_ids {
        match skillstar_skills::deployment::batch_link_skills_to_agent(skill_names, &agent_id) {
            Ok(_) => deployed_to.push(agent_id),
            Err(err) => failures.push(format!("{agent_id}: {err:#}")),
        }
    }

    if failures.is_empty() {
        Ok(deployed_to)
    } else {
        Err(format!(
            "Installed to the hub but failed to deploy to {} Agent(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use skillstar_agents::AgentProfile;

    fn profile(id: &str, enabled: bool, global_skills: bool) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            display_name: id.to_string(),
            icon: String::new(),
            global_skills_dir: if global_skills {
                std::path::PathBuf::from(format!("/tmp/{id}/skills"))
            } else {
                std::path::PathBuf::new()
            },
            project_skills_rel: String::new(),
            installed: true,
            enabled,
            synced_count: 0,
        }
    }

    #[test]
    fn filters_enabled_global_profiles_only() {
        let profiles = vec![
            profile("codex", true, true),
            profile("grok", true, false),
            profile("opencode", false, true),
            profile("claude", true, true),
        ];
        let enabled = profiles
            .iter()
            .filter(|p| p.enabled && p.has_global_skills())
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(enabled, vec!["codex", "claude"]);
    }
}
