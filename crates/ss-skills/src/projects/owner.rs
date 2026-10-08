//! One manifest owner and one member set for a shared project skill directory.
//!
//! Existing manifest keys win. Otherwise the selected agent owns the path.
//! This module does not invent an agent when the selection is empty, and it
//! does not write the manifest.

use crate::agents::{AgentProfile, additional_project_skill_reads};

use super::types::{ProjectDeployMode, SkillsList};

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

/// Owner, readers, member union, and deploy mode for one physical path.
///
/// `members` is every name already stored under an agent on this path, in
/// profile order, followed by `incoming`. The manifest is not modified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedPathMembership {
    pub owner_id: Option<String>,
    pub readers: Vec<String>,
    pub members: Vec<String>,
    pub mode: Option<ProjectDeployMode>,
}

pub(crate) fn shared_path_membership(
    profiles: &[AgentProfile],
    skills_list: &SkillsList,
    project_skills_rel: &str,
    selected_agent_id: &str,
    incoming: &[String],
) -> SharedPathMembership {
    let SharedPathOwner { owner_id, readers } =
        shared_path_owner(profiles, skills_list, project_skills_rel, selected_agent_id);
    let mut members = Vec::new();
    for profile in profiles {
        if profile.project_skills_rel != project_skills_rel {
            continue;
        }
        if let Some(skills) = skills_list.agents.get(&profile.id) {
            push_names(&mut members, skills);
        }
    }
    push_names(&mut members, incoming);
    SharedPathMembership {
        owner_id,
        readers,
        members,
        mode: skills_list.deploy_modes.get(project_skills_rel).copied(),
    }
}

/// Move every agent list on this physical path onto the manifest owner and
/// append `incoming`. Sibling keys are removed. Deploy mode is left alone,
/// and nothing is written to disk.
///
/// Returns the owner id that now holds the union. When the path has no
/// manifest key and `selected_agent_id` is empty, the list is unchanged.
pub(crate) fn collapse_shared_path(
    skills_list: &mut SkillsList,
    profiles: &[AgentProfile],
    project_skills_rel: &str,
    selected_agent_id: &str,
    incoming: &[String],
) -> Option<String> {
    let membership = shared_path_membership(
        profiles,
        skills_list,
        project_skills_rel,
        selected_agent_id,
        incoming,
    );
    let owner_id = membership.owner_id?;
    for profile in profiles {
        if profile.project_skills_rel == project_skills_rel {
            skills_list.agents.remove(&profile.id);
        }
    }
    skills_list
        .agents
        .insert(owner_id.clone(), membership.members);
    Some(owner_id)
}

fn push_names(members: &mut Vec<String>, names: &[String]) {
    for name in names {
        if !members.iter().any(|existing| existing == name) {
            members.push(name.clone());
        }
    }
}

fn push_unique(readers: &mut Vec<String>, id: String) {
    if !readers.iter().any(|existing| existing == &id) {
        readers.push(id);
    }
}

#[cfg(test)]
mod shared_path_owner_tests {
    use super::{collapse_shared_path, shared_path_membership, shared_path_owner};
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

    #[test]
    fn shared_path_membership_keeps_one_owner_and_every_name() {
        let profiles = vec![
            profile("codex", ".agents/skills"),
            profile("opencode", ".agents/skills"),
            profile("claude", ".claude/skills"),
        ];
        let mut list = SkillsList::default();
        list.agents.insert(
            "codex".to_string(),
            vec!["alpha".to_string(), "gamma".to_string()],
        );
        list.agents.insert(
            "opencode".to_string(),
            vec!["beta".to_string(), "alpha".to_string()],
        );
        list.agents
            .insert("claude".to_string(), vec!["other".to_string()]);

        let seen = shared_path_membership(
            &profiles,
            &list,
            ".agents/skills",
            "opencode",
            &["delta".to_string()],
        );
        assert_eq!(seen.owner_id.as_deref(), Some("codex"));
        assert_eq!(
            seen.members,
            vec![
                "alpha".to_string(),
                "gamma".to_string(),
                "beta".to_string(),
                "delta".to_string()
            ]
        );

        let owner = collapse_shared_path(
            &mut list,
            &profiles,
            ".agents/skills",
            "opencode",
            &["delta".to_string()],
        );
        assert_eq!(owner.as_deref(), Some("codex"));
        assert_eq!(
            list.agents.get("codex"),
            Some(&vec![
                "alpha".to_string(),
                "gamma".to_string(),
                "beta".to_string(),
                "delta".to_string()
            ])
        );
        assert!(!list.agents.contains_key("opencode"));
        assert_eq!(list.agents.get("claude"), Some(&vec!["other".to_string()]));

        let again = collapse_shared_path(
            &mut list,
            &profiles,
            ".agents/skills",
            "opencode",
            &["later".to_string()],
        );
        assert_eq!(again.as_deref(), Some("codex"));
        assert_eq!(
            list.agents.get("codex").map(Vec::as_slice),
            Some(
                [
                    "alpha".to_string(),
                    "gamma".to_string(),
                    "beta".to_string(),
                    "delta".to_string(),
                    "later".to_string()
                ]
                .as_slice()
            )
        );
    }

    #[test]
    fn shared_path_membership_uses_the_selected_agent_when_unowned() {
        let profiles = vec![
            profile("codex", ".agents/skills"),
            profile("opencode", ".agents/skills"),
        ];
        let mut unowned = SkillsList::default();
        assert!(
            collapse_shared_path(
                &mut unowned,
                &profiles,
                ".agents/skills",
                "",
                &["extra".to_string()]
            )
            .is_none()
        );
        assert!(unowned.agents.is_empty());

        let mut list = SkillsList::default();
        let owner = collapse_shared_path(
            &mut list,
            &profiles,
            ".agents/skills",
            "opencode",
            &["new-one".to_string()],
        );
        assert_eq!(owner.as_deref(), Some("opencode"));
        assert_eq!(
            list.agents.get("opencode"),
            Some(&vec!["new-one".to_string()])
        );
        assert!(!list.agents.contains_key("codex"));
    }
}
