//! One manifest owner for a shared project skill directory.
//!
//! Existing manifest keys win. Otherwise the selected agent owns the path.
//! This function does not invent an agent when the selection is empty.

use crate::agents::{AgentProfile, additional_project_skill_reads};

use super::types::SkillsList;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedPathOwner {
    pub owner_id: Option<String>,
    /// Agents that will see this directory, sorted by id.
    pub readers: Vec<String>,
}

pub fn shared_path_owner(
    profiles: &[AgentProfile],
    skills_list: &SkillsList,
    project_skills_rel: &str,
    selected_agent_id: &str,
) -> SharedPathOwner {
    let owner_id = profiles
        .iter()
        .find(|candidate| {
            candidate.project_skills_rel == project_skills_rel
                && skills_list.agents.contains_key(&candidate.id)
        })
        .map(|candidate| candidate.id.clone())
        .or_else(|| {
            if selected_agent_id.is_empty() {
                None
            } else {
                Some(selected_agent_id.to_string())
            }
        });

    let mut readers = Vec::new();
    for profile in profiles {
        if profile.has_project_skills() && profile.project_skills_rel == project_skills_rel {
            push_unique(&mut readers, profile.id.clone());
        }
    }
    for (agent_id, extra_rel) in additional_project_skill_reads() {
        if *extra_rel == project_skills_rel {
            push_unique(&mut readers, (*agent_id).to_string());
        }
    }
    readers.sort();
    SharedPathOwner { owner_id, readers }
}

fn push_unique(readers: &mut Vec<String>, id: String) {
    if !readers.iter().any(|existing| existing == &id) {
        readers.push(id);
    }
}

#[cfg(test)]
mod shared_path_owner_tests {
    use super::shared_path_owner;
    use crate::agents::AgentProfile;
    use crate::projects::SkillsList;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn profile(id: &str, rel: &str) -> AgentProfile {
        AgentProfile {
            id: id.to_string(),
            display_name: id.to_string(),
            icon: String::new(),
            global_skills_dir: PathBuf::new(),
            project_skills_rel: rel.to_string(),
            installed: false,
            enabled: true,
            synced_count: 0,
        }
    }

    fn list_with(owner: &str) -> SkillsList {
        let mut agents = HashMap::new();
        agents.insert(owner.to_string(), vec!["demo".to_string()]);
        SkillsList {
            agents,
            ..SkillsList::default()
        }
    }

    #[test]
    fn shared_path_owner_preserves_existing_owner() {
        let profiles = vec![
            profile("codex", ".agents/skills"),
            profile("opencode", ".agents/skills"),
        ];
        let decision =
            shared_path_owner(&profiles, &list_with("opencode"), ".agents/skills", "codex");
        assert_eq!(decision.owner_id.as_deref(), Some("opencode"));
    }

    #[test]
    fn shared_path_owner_uses_requested_agent_when_unowned() {
        let profiles = vec![
            profile("codex", ".agents/skills"),
            profile("opencode", ".agents/skills"),
        ];
        let decision = shared_path_owner(
            &profiles,
            &SkillsList::default(),
            ".agents/skills",
            "opencode",
        );
        assert_eq!(decision.owner_id.as_deref(), Some("opencode"));
        assert!(
            shared_path_owner(&profiles, &SkillsList::default(), ".agents/skills", "")
                .owner_id
                .is_none()
        );
    }

    #[test]
    fn shared_path_disclosure_includes_deepseek_for_agents_skills() {
        let profiles = vec![
            profile("codex", ".agents/skills"),
            profile("deepseek", ".dsh/skills"),
        ];
        let decision =
            shared_path_owner(&profiles, &SkillsList::default(), ".agents/skills", "codex");
        assert!(decision.readers.iter().any(|id| id == "codex"));
        assert!(decision.readers.iter().any(|id| id == "deepseek"));
        assert_eq!(
            shared_path_owner(&profiles, &SkillsList::default(), ".dsh/skills", "deepseek").readers,
            vec!["deepseek".to_string()]
        );
    }
}
