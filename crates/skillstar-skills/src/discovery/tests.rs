use super::*;

fn write_skill_md(path: &Path, name: &str, description: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n",);
    std::fs::write(path, content)
}

#[test]
fn discover_root_first_returns_only_root_skill() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");

    write_skill_md(&repo.join("SKILL.md"), "root-skill", "root").unwrap();
    write_skill_md(
        &repo.join("skills/nested-skill/SKILL.md"),
        "nested-skill",
        "nested",
    )
    .unwrap();

    let skills = discover_skills(&repo, false);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "root-skill");
    assert!(skills[0].folder_path.is_empty());
}

#[test]
fn discover_full_depth_includes_all_skills() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");

    write_skill_md(&repo.join("SKILL.md"), "root-skill", "root").unwrap();
    write_skill_md(
        &repo.join("skills/nested-skill/SKILL.md"),
        "nested-skill",
        "nested",
    )
    .unwrap();

    let skills = discover_skills(&repo, true);
    assert_eq!(skills.len(), 2);
    assert!(
        skills
            .iter()
            .any(|s| s.id == "root-skill" && s.folder_path.is_empty())
    );
    assert!(
        skills
            .iter()
            .any(|s| s.id == "nested-skill" && s.folder_path == "skills/nested-skill")
    );
}

#[test]
fn discover_uses_frontmatter_name() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");
    write_skill_md(&repo.join("SKILL.md"), "custom-name", "desc").unwrap();

    let skills = discover_skills(&repo, false);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "custom-name");
}

#[test]
fn discover_root_default_name_keeps_repo_double_dash_segments() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--my--tool");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("SKILL.md"), "# demo\n").unwrap();

    let skills = discover_skills(&repo, false);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "my--tool");
}

#[test]
fn discover_deduplicates_agent_copies() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(
        &repo.join("source/skills/my-skill/SKILL.md"),
        "my-skill",
        "canonical",
    )
    .unwrap();
    write_skill_md(
        &repo.join(".claude/skills/my-skill/SKILL.md"),
        "my-skill",
        "claude copy",
    )
    .unwrap();

    let skills = discover_skills(repo, true);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "my-skill");
    assert!(skills[0].folder_path.starts_with("source/skills"));
}

#[test]
fn discover_empty_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let skills = discover_skills(dir.path(), false);
    assert!(skills.is_empty());
}

#[test]
fn source_priority_ordering() {
    assert!(source_priority("source/skills/foo") > source_priority(".agents/skills/foo"));
    assert!(source_priority(".agents/skills/foo") > source_priority(".claude/skills/foo"));
    // Singular `.agent/skills` (Antigravity CLI official path) ranks the
    // same as the legacy plural form.
    assert_eq!(
        source_priority(".agent/skills/foo"),
        source_priority(".agents/skills/foo")
    );
}

#[test]
fn dedupe_keeps_higher_priority() {
    let skills = vec![
        DiscoveredSkill {
            id: "my-skill".to_string(),
            folder_path: ".claude/skills/my-skill".to_string(),
            description: "low priority".to_string(),
            already_installed: false,
            frontmatter_issues: Vec::new(),
        },
        DiscoveredSkill {
            id: "my-skill".to_string(),
            folder_path: "source/skills/my-skill".to_string(),
            description: "high priority".to_string(),
            already_installed: false,
            frontmatter_issues: Vec::new(),
        },
    ];
    let deduped = dedupe_discovered_skills(skills);
    assert_eq!(deduped.len(), 1);
    assert!(deduped[0].folder_path.starts_with("source/skills"));
}

#[test]
fn discover_priority_dir_skips_non_standard() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(
        &repo.join("skills/opencli-browser/SKILL.md"),
        "opencli-browser",
        "browser",
    )
    .unwrap();
    write_skill_md(
        &repo.join("clis/antigravity/SKILL.md"),
        "antigravity",
        "desktop automation",
    )
    .unwrap();

    let skills = discover_skills(repo, false);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "opencli-browser");
}

#[test]
fn discover_falls_back_to_non_standard_when_no_priority_skills() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(&repo.join("custom/demo/SKILL.md"), "demo", "non-standard").unwrap();

    let skills = discover_skills(repo, false);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].id, "demo");
}

#[test]
fn discover_full_depth_includes_non_standard() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(
        &repo.join("skills/opencli-browser/SKILL.md"),
        "opencli-browser",
        "browser",
    )
    .unwrap();
    write_skill_md(
        &repo.join("clis/antigravity/SKILL.md"),
        "antigravity",
        "desktop automation",
    )
    .unwrap();

    let skills = discover_skills(repo, true);
    assert_eq!(skills.len(), 2);
    assert!(
        skills
            .iter()
            .any(|s| s.id == "opencli-browser" && s.folder_path == "skills/opencli-browser")
    );
    assert!(
        skills
            .iter()
            .any(|s| s.id == "antigravity" && s.folder_path == "clis/antigravity")
    );
}

#[cfg(unix)]
#[test]
fn discovery_never_follows_repository_symlinks() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let outside = dir.path().join("outside");
    write_skill_md(&outside.join("nested/SKILL.md"), "secret", "outside").unwrap();
    std::fs::create_dir_all(repo.join("skills")).unwrap();
    symlink(outside.join("nested"), repo.join("skills/leak")).unwrap();
    symlink(outside.join("nested/SKILL.md"), repo.join("SKILL.md")).unwrap();

    assert!(discover_skills(&repo, true).is_empty());
    assert!(discover_skills(&repo, false).is_empty());
}

#[cfg(unix)]
#[test]
fn priority_discovery_rejects_symlinked_parent_directories() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let outside = dir.path().join("outside");
    write_skill_md(&outside.join("skills/secret/SKILL.md"), "secret", "outside").unwrap();
    std::fs::create_dir_all(repo.join(".agents")).unwrap();
    symlink(outside.join("skills"), repo.join(".agents/skills")).unwrap();

    assert!(discover_skills(&repo, false).is_empty());

    let repo_with_symlinked_root = dir.path().join("repo-parent");
    std::fs::create_dir_all(&repo_with_symlinked_root).unwrap();
    symlink(&outside, repo_with_symlinked_root.join(".agents")).unwrap();
    assert!(discover_skills(&repo_with_symlinked_root, false).is_empty());
}

#[test]
fn discovery_rejects_oversized_skill_manifests() {
    let dir = tempfile::tempdir().unwrap();
    let skill_md = dir.path().join("SKILL.md");
    std::fs::write(&skill_md, vec![b'x'; 1_048_577]).unwrap();

    assert!(discover_skills(dir.path(), true).is_empty());
}

#[test]
fn skill_discovery_pipeline_matches_compatibility_api() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(&repo.join("skills/demo/SKILL.md"), "demo", "demo").unwrap();

    let from_pipeline = SkillDiscovery::new(repo, false).discover();
    let from_compat = discover_skills(repo, false);

    assert_eq!(from_pipeline.len(), 1);
    assert_eq!(from_pipeline[0].id, from_compat[0].id);
    assert_eq!(from_pipeline[0].folder_path, from_compat[0].folder_path);
    assert_eq!(from_pipeline[0].description, from_compat[0].description);
}

#[test]
fn skill_discovery_candidate_keeps_root_path_and_frontmatter() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");
    write_skill_md(&repo.join("SKILL.md"), "root-name", "root-desc").unwrap();

    let discovery = SkillDiscovery::new(&repo, false);
    let candidates = discovery.collect_candidates();

    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].is_repo_root());
    assert_eq!(candidates[0].default_name, "repo");
    assert_eq!(candidates[0].frontmatter.name.as_deref(), Some("root-name"));
    assert_eq!(candidates[0].frontmatter.description, "root-desc");
}

#[test]
fn resolve_install_skills_picks_cursor_or_dsh_harness_folder() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("rust-skills");
    write_skill_md(&repo.join("skills/rust/SKILL.md"), "rust", "catalog").unwrap();
    write_skill_md(
        &repo.join(".cursor/skills/rust/SKILL.md"),
        "rust",
        "cursor copy",
    )
    .unwrap();
    write_skill_md(&repo.join(".dsh/skills/rust/SKILL.md"), "rust", "dsh copy").unwrap();

    let catalog = resolve_install_skills(&repo, Some("rust"), None, None).unwrap();
    assert_eq!(catalog[0].folder_path, "skills/rust");

    let cursor = resolve_install_skills(&repo, Some("rust"), Some(".cursor"), None).unwrap();
    assert_eq!(cursor[0].folder_path, ".cursor/skills/rust");

    let dsh = resolve_install_skills(&repo, Some("rust"), Some(".dsh"), None).unwrap();
    assert_eq!(dsh[0].folder_path, ".dsh/skills/rust");
}

#[test]
fn resolve_install_skills_falls_back_when_the_clicked_harness_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("impeccable");
    write_skill_md(
        &repo.join("skills/impeccable/SKILL.md"),
        "impeccable",
        "catalog",
    )
    .unwrap();
    write_skill_md(
        &repo.join(".cursor/skills/impeccable/SKILL.md"),
        "impeccable",
        "cursor copy",
    )
    .unwrap();

    let catalog = resolve_install_skills(&repo, Some("impeccable"), Some(".dsh"), None).unwrap();
    assert_eq!(catalog[0].folder_path, "skills/impeccable");

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("impeccable-no-catalog");
    write_skill_md(
        &repo.join(".cursor/skills/impeccable/SKILL.md"),
        "impeccable",
        "cursor copy",
    )
    .unwrap();
    write_skill_md(
        &repo.join(".agents/skills/impeccable/SKILL.md"),
        "impeccable",
        "codex copy",
    )
    .unwrap();

    let preferred = resolve_install_skills(
        &repo,
        Some("impeccable"),
        Some(".dsh"),
        Some(".cursor/skills/impeccable"),
    )
    .unwrap();
    assert_eq!(preferred[0].folder_path, ".cursor/skills/impeccable");

    let other = resolve_install_skills(&repo, Some("impeccable"), Some(".dsh"), None).unwrap();
    assert_eq!(other[0].folder_path, ".agents/skills/impeccable");
    assert!(!other[0].folder_path.is_empty());
}

#[test]
fn resolve_install_skills_fails_only_without_a_nested_skill_payload() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("root-only");
    write_skill_md(&repo.join("SKILL.md"), "solo", "root only").unwrap();

    let error = resolve_install_skills(&repo, Some("solo"), Some(".dsh"), None).unwrap_err();
    assert!(error.contains("no installable SKILL.md"), "{error}");
    assert!(
        error.contains("repository root is not an install unit"),
        "{error}"
    );
}

#[test]
fn select_harness_skill_keeps_agent_and_agents_distinct() {
    let skills = vec![
        DiscoveredSkill {
            id: "impeccable".to_string(),
            folder_path: ".agent/skills/impeccable".to_string(),
            description: "antigravity".to_string(),
            already_installed: false,
            frontmatter_issues: Vec::new(),
        },
        DiscoveredSkill {
            id: "impeccable".to_string(),
            folder_path: ".agents/skills/impeccable".to_string(),
            description: "codex".to_string(),
            already_installed: false,
            frontmatter_issues: Vec::new(),
        },
    ];
    assert_eq!(
        select_harness_skill(&skills, ".agent").map(|skill| skill.folder_path.as_str()),
        Some(".agent/skills/impeccable")
    );
    assert_eq!(
        select_harness_skill(&skills, ".agents").map(|skill| skill.folder_path.as_str()),
        Some(".agents/skills/impeccable")
    );
}
