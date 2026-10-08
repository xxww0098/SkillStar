//! Deck-owned Agent rails.
//!
//! A deck owns the set of Agents it is linked to (`SkillGroup::agent_links`).
//! Creating a deck does not link its Skills anywhere; the rail stays dark
//! until the user links an Agent. Installing a Skill also does not link it.
//! A missing `agent_links` field displays as unlinked. This module lights or
//! clears one Agent, and deploys a deck to every enabled global Agent.

use std::collections::HashSet;

use super::agent_links::{AgentLinkReport, enabled_global_agent_ids};
use crate::skill_group::{self, SkillGroup};

/// Light or clear one Agent on a deck's rail: link (or unlink) every installed
/// deck Skill there, then record the rail. The rail keeps its old state for
/// that Agent when every link attempt failed or was skipped.
pub fn set_deck_agent(
    group_id: &str,
    agent_id: &str,
    enable: bool,
) -> anyhow::Result<AgentLinkReport> {
    let group = find_group(group_id)?;
    let mut report = AgentLinkReport::default();
    for skill in installed_deck_skills(&group) {
        report.link(&skill, agent_id, enable);
    }
    if report.rail_may_change(agent_id) {
        let mut links = group.agent_links.unwrap_or_default();
        links.retain(|id| id != agent_id);
        if enable {
            links.push(agent_id.to_string());
        }
        skill_group::update_group(group.id, None, None, None, None, None, Some(links))?;
    }
    Ok(report)
}

/// "Deploy to all": link the deck into every enabled global Agent and add each
/// Agent that received at least one Skill to the rail.
pub fn link_deck_to_enabled_agents(group_id: &str) -> anyhow::Result<AgentLinkReport> {
    let group = find_group(group_id)?;
    let skills = installed_deck_skills(&group);
    let mut report = AgentLinkReport::default();
    let mut links = group.agent_links.clone().unwrap_or_default();
    for agent_id in enabled_global_agent_ids() {
        for skill in &skills {
            report.link(skill, &agent_id, true);
        }
        if report.rail_may_change(&agent_id) && !links.contains(&agent_id) {
            links.push(agent_id);
        }
    }
    skill_group::update_group(group.id, None, None, None, None, None, Some(links))?;
    Ok(report)
}

fn find_group(group_id: &str) -> anyhow::Result<SkillGroup> {
    skill_group::list_groups()
        .into_iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| anyhow::anyhow!("Deck '{group_id}' not found"))
}

/// Deck Skills present in the canonical root, as their on-disk folder names.
fn installed_deck_skills(group: &SkillGroup) -> Vec<String> {
    let installed = crate::installer::installed_names()
        .into_iter()
        .map(|name| (key(&name), name))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen = HashSet::new();
    group
        .skills
        .iter()
        .filter_map(|name| installed.get(&key(name)).cloned())
        .filter(|name| seen.insert(name.clone()))
        .collect()
}

/// Deck skill names are stored as the user typed them; disk and lockfile
/// markers are lowercased. Compare on one form.
fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}
