//! Deck search predicate. Kept apart from the page so it can be tested as
//! plain data, without a GPUI context.

use ss_skills::skill_group::SkillGroup;

/// Check if a skill deck matches a user search query across name, description, or contained skills.
pub fn matches_deck_query(deck: &SkillGroup, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    deck.name.to_lowercase().contains(&query)
        || deck.description.to_lowercase().contains(&query)
        || deck
            .skills
            .iter()
            .any(|s| s.to_lowercase().contains(&query))
}

#[cfg(test)]
mod tests {
    use super::matches_deck_query;
    use ss_skills::skill_group::SkillGroup;

    #[test]
    fn test_matches_deck_query_name_and_skills() {
        let deck = SkillGroup {
            id: "deck-1".into(),
            name: "Frontend Starter".into(),
            description: "Essential tools for React and CSS".into(),
            icon: "🎨".into(),
            skills: vec!["tailwind-helper".into(), "react-hooks".into()],
            skill_sources: Default::default(),
            default_agent: String::new(),
            agent_links: Default::default(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };

        assert!(matches_deck_query(&deck, ""));
        assert!(matches_deck_query(&deck, "Front"));
        assert!(matches_deck_query(&deck, "starter"));
        assert!(matches_deck_query(&deck, "react"));
        assert!(matches_deck_query(&deck, "tailwind"));
        assert!(!matches_deck_query(&deck, "python"));
    }
}
