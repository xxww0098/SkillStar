//! The page's query surface: search, agent, source, repo, updates, and sort.
//!
//! These read page state and hand back owned rows, so the render tree only
//! calls them. They live outside `mod.rs` to keep that file on state,
//! lifecycle, and the frame.

use ss_core::types::skill::{Skill, SkillType};

use super::MySkillsPage;
use super::types::{SortOption, SourceFilter};

impl MySkillsPage {
    /// Search, agent, source, and repo — the set the toolbar counts before
    /// the updates toggle, matching `scopedSkills` in `LocalSkillsContent`.
    pub fn scoped_skills(&self) -> Vec<Skill> {
        let query = self.search_query.to_lowercase();
        let agent_name = self.agent_filter.as_ref().and_then(|id| {
            self.profiles
                .iter()
                .find(|profile| &profile.id == id)
                .map(|profile| profile.display_name.clone())
        });
        self.skills
            .iter()
            .filter(|skill| {
                skill_in_scope(
                    skill,
                    &query,
                    &self.source_filter,
                    &self.repo_filter,
                    agent_name.as_deref(),
                )
            })
            .cloned()
            .collect()
    }

    /// Returns skills matching the current search, source filter, updates filter, and sort.
    pub fn filtered_skills(&self) -> Vec<Skill> {
        let mut filtered: Vec<Skill> = self
            .scoped_skills()
            .into_iter()
            .filter(|skill| {
                !self.only_updates || skill.update_available || skill.upstream_change.is_some()
            })
            .collect();

        match self.sort_by {
            SortOption::Updated => {
                filtered.sort_by(|a, b| {
                    b.last_updated
                        .cmp(&a.last_updated)
                        .then_with(|| a.name.cmp(&b.name))
                });
            }
            SortOption::Name => {
                filtered.sort_by(|a, b| a.name.cmp(&b.name));
            }
            SortOption::Stars => {
                filtered.sort_by(|a, b| b.stars.cmp(&a.stars).then_with(|| a.name.cmp(&b.name)));
            }
        }

        filtered
    }
}

fn skill_in_scope(
    skill: &Skill,
    query: &str,
    source: &SourceFilter,
    repo: &Option<String>,
    agent_name: Option<&str>,
) -> bool {
    if !query.is_empty() {
        let name_match = skill.name.to_lowercase().contains(query);
        let desc_match = skill.description.to_lowercase().contains(query);
        let loc_match = skill
            .localized_description
            .as_deref()
            .is_some_and(|text| text.to_lowercase().contains(query));
        let source_match = skill
            .source
            .as_deref()
            .is_some_and(|text| text.to_lowercase().contains(query));
        let author_match = skill
            .author
            .as_deref()
            .is_some_and(|text| text.to_lowercase().contains(query));
        if !name_match && !desc_match && !loc_match && !source_match && !author_match {
            return false;
        }
    }
    match source {
        SourceFilter::All => {}
        SourceFilter::Hub if skill.skill_type == SkillType::Local => return false,
        SourceFilter::Local if skill.skill_type != SkillType::Local => return false,
        SourceFilter::Hub | SourceFilter::Local => {}
    }
    if repo
        .as_ref()
        .is_some_and(|repo| skill.source.as_deref() != Some(repo))
    {
        return false;
    }
    if let Some(name) = agent_name {
        let linked = skill
            .agent_links
            .as_ref()
            .is_some_and(|links| links.iter().any(|link| link == name));
        if !linked {
            return false;
        }
    }
    true
}
