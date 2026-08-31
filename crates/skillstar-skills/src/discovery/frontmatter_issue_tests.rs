use super::*;

#[test]
fn discovery_reports_frontmatter_issues_on_invalid_skills() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");
    std::fs::create_dir_all(&repo).unwrap();
    // Valid skill — no issues.
    std::fs::create_dir_all(repo.join("skills/valid")).unwrap();
    std::fs::write(
        repo.join("skills/valid/SKILL.md"),
        "---\nname: valid\ndescription: A valid skill\n---\n",
    )
    .unwrap();
    // Bare SKILL.md — missing frontmatter/name/description.
    std::fs::create_dir_all(repo.join("skills/bare")).unwrap();
    std::fs::write(repo.join("skills/bare/SKILL.md"), "# No frontmatter\n").unwrap();

    let skills = discover_skills(&repo, true);
    let valid = skills.iter().find(|s| s.id == "valid").unwrap();
    assert!(
        valid.frontmatter_issues.is_empty(),
        "{:?}",
        valid.frontmatter_issues
    );

    let bare = skills.iter().find(|s| s.id == "bare").unwrap();
    assert!(
        bare.frontmatter_issues
            .contains(&"missing_description".to_string())
    );
    assert!(
        bare.frontmatter_issues
            .contains(&"missing_frontmatter".to_string())
    );
}
