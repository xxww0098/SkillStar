use super::{derive_name_hint, find_target_skill, requested_skill_not_found_error};
use crate::repo_scanner::DiscoveredSkill;

fn discovered(id: &str) -> DiscoveredSkill {
    DiscoveredSkill {
        id: id.to_string(),
        folder_path: format!("skills/{id}"),
        description: String::new(),
        already_installed: false,
    }
}

#[test]
fn derive_name_hint_prefers_explicit_name() {
    let hint = derive_name_hint(
        "https://github.com/example/skills.git",
        Some("explicit-name"),
    );
    assert_eq!(hint, "explicit-name");
}

#[test]
fn derive_name_hint_falls_back_to_repo_tail() {
    let hint = derive_name_hint("https://github.com/example/awesome-skill.git", None);
    assert_eq!(hint, "awesome-skill");
}

#[test]
fn find_target_skill_prefers_requested_name_case_insensitive() {
    let skills = vec![discovered("frontend-ui"), discovered("security-review")];
    let target = find_target_skill(&skills, Some("FRONTEND-UI"), "unused-name-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Some("frontend-ui"));
}

#[test]
fn find_target_skill_uses_single_skill_fallback() {
    let skills = vec![discovered("only-one")];
    let target = find_target_skill(&skills, None, "no-match-hint");
    assert_eq!(target.map(|skill| skill.id.as_str()), Some("only-one"));
}

#[test]
fn find_target_skill_rejects_different_single_skill_when_name_is_explicit() {
    let skills = vec![discovered("renamed-skill")];
    let target = find_target_skill(&skills, Some("removed-skill"), "removed-skill");
    assert!(target.is_none());
}

#[test]
fn requested_skill_not_found_error_names_missing_identity_and_possible_cause() {
    let error = requested_skill_not_found_error(&["removed-skill".to_string()]);
    assert!(error.contains("removed-skill"));
    assert!(error.contains("deleted or renamed"));
}
