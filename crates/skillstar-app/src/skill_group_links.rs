//! Cross-domain resolution of deck-owned Agent links.
//!
//! A deck owns the set of Agents it is linked to (`SkillGroup::agent_links`),
//! so a freshly created deck lights up no Agent even when its Skills are
//! already deployed globally by the install-time step in [`crate::global_deploy`].
//!
//! Decks written before that field existed carry `None`. Dropping them to "no
//! Agent" would silently retract a rail the user had already lit, so this
//! module resolves `None` once from the on-disk Agent state — a deck counts as
//! linked to an Agent when every installed Skill it holds is linked there,
//! which is exactly the rule the UI used to derive the rail — and persists the
//! result. It lives in `skillstar-app` because that resolution reads the
//! Agents, Skills and deck domains together.

use std::collections::HashSet;

use skillstar_skills::skill_group::{self, SkillGroup};

/// Deck list with every deck's Agent links resolved.
///
/// Backfill failures are logged and swallowed: a deck list is a read path, and
/// an unwritable store must not blank the Decks page. The next call retries.
pub fn list_groups_with_agent_links() -> Vec<SkillGroup> {
    let mut groups = skill_group::list_groups();
    if groups.iter().all(|group| group.agent_links.is_some()) {
        return groups;
    }

    let agents = linked_skills_per_agent();
    let installed = skillstar_skills::installed_skill::installed_snapshot_markers();
    let resolved = groups
        .iter()
        .filter(|group| group.agent_links.is_none())
        .map(|group| {
            (
                group.id.clone(),
                derive_agent_links(&group.skills, &installed, &agents),
            )
        })
        .collect::<Vec<_>>();

    if let Err(err) = skill_group::backfill_agent_links(&resolved) {
        tracing::warn!(target: "skills", error = %err, "Failed to persist deck Agent-link backfill");
    }

    let resolved: std::collections::HashMap<_, _> = resolved.into_iter().collect();
    for group in &mut groups {
        if group.agent_links.is_none() {
            group.agent_links = resolved
                .get(&group.id)
                .cloned()
                .or_else(|| Some(Vec::new()));
        }
    }
    groups
}

/// `(agent_id, linked skill names)` for every enabled Agent that takes global
/// Skills — the same population the deck rail offers as targets.
fn linked_skills_per_agent() -> Vec<(String, HashSet<String>)> {
    skillstar_skills::agents::list_profiles()
        .into_iter()
        .filter(|profile| profile.enabled && profile.has_global_skills())
        .filter_map(
            |profile| match skillstar_skills::deployment::list_linked_skills(&profile.id) {
                Ok(names) => Some((profile.id, names.iter().map(|n| key(n)).collect())),
                Err(err) => {
                    tracing::warn!(
                        target: "skills",
                        agent_id = %profile.id,
                        error = %err,
                        "Skipping Agent while backfilling deck links"
                    );
                    None
                }
            },
        )
        .collect()
}

/// Agents a legacy deck was effectively on: every installed Skill of the deck
/// is linked there. A deck with no installed Skill resolves to no Agent — there
/// is nothing on disk that could have lit its rail.
fn derive_agent_links(
    deck_skills: &[String],
    installed: &HashSet<String>,
    agents: &[(String, HashSet<String>)],
) -> Vec<String> {
    let mut seen = HashSet::new();
    let deck_installed = deck_skills
        .iter()
        .map(|name| key(name))
        .filter(|name| !name.is_empty() && installed.contains(name))
        .filter(|name| seen.insert(name.clone()))
        .collect::<Vec<_>>();

    if deck_installed.is_empty() {
        return Vec::new();
    }

    agents
        .iter()
        .filter(|(_, linked)| deck_installed.iter().all(|name| linked.contains(name)))
        .map(|(agent_id, _)| agent_id.clone())
        .collect()
}

/// Deck skill names are stored as the user typed them; disk and lockfile
/// markers are lowercased. Compare on one form.
fn key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents() -> Vec<(String, HashSet<String>)> {
        vec![
            (
                "claude".to_string(),
                ["git-flow", "pdf-tools"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            (
                "codex".to_string(),
                ["git-flow"].iter().map(|s| s.to_string()).collect(),
            ),
        ]
    }

    fn installed() -> HashSet<String> {
        ["git-flow", "pdf-tools", "xlsx"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn keeps_only_agents_holding_every_installed_deck_skill() {
        let links = derive_agent_links(
            &["git-flow".into(), "pdf-tools".into()],
            &installed(),
            &agents(),
        );
        assert_eq!(links, vec!["claude".to_string()]);
    }

    #[test]
    fn ignores_skills_missing_from_the_hub() {
        // `not-installed` is in the deck but absent from the hub, so it must not
        // hold back an Agent that has every installed deck Skill.
        let links = derive_agent_links(
            &["git-flow".into(), "not-installed".into()],
            &installed(),
            &agents(),
        );
        assert_eq!(links, vec!["claude".to_string(), "codex".to_string()]);
    }

    #[test]
    fn matches_names_case_insensitively() {
        let links = derive_agent_links(&[" Git-Flow ".into()], &installed(), &agents());
        assert_eq!(links, vec!["claude".to_string(), "codex".to_string()]);
    }

    #[test]
    fn deck_without_installed_skills_claims_no_agent() {
        let links = derive_agent_links(&["ghost".into()], &installed(), &agents());
        assert!(links.is_empty());
    }
}
