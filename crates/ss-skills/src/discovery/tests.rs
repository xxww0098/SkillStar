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
fn discover_reports_internal_and_accepts_bom() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");
    std::fs::create_dir_all(repo.join("skills/hidden")).unwrap();
    std::fs::write(
        repo.join("skills/hidden/SKILL.md"),
        "---\nname: hidden\ndescription: Helper\nmetadata:\n  internal: true\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("skills/bom-skill")).unwrap();
    std::fs::write(
        repo.join("skills/bom-skill/SKILL.md"),
        "\u{feff}---\nname: bom-skill\ndescription: Has a BOM\n---\n",
    )
    .unwrap();

    let skills = discover_skills(&repo, true);
    let hidden = skills.iter().find(|skill| skill.id == "hidden").unwrap();
    assert!(hidden.internal);
    assert!(hidden.installable);
    assert!(!hidden.included_in_bulk());
    let bom = skills.iter().find(|skill| skill.id == "bom-skill").unwrap();
    assert!(!bom.internal);
    assert!(bom.installable);
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
    // vercel parity: a copy inside a priority container (.claude/skills)
    // beats one in a non-container path (source/skills) — first-seen in
    // priority order wins, there is no catalog ranking table.
    assert_eq!(skills[0].folder_path, ".claude/skills/my-skill");
}

#[test]
fn discover_empty_dir_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let skills = discover_skills(dir.path(), false);
    assert!(skills.is_empty());
}

#[test]
fn gemini_skills_is_a_priority_container() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(
        &dir.path().join(".gemini/skills/banner/SKILL.md"),
        "banner",
        "gemini",
    )
    .unwrap();
    write_skill_md(
        &dir.path().join("examples/other/SKILL.md"),
        "other",
        "noise",
    )
    .unwrap();

    let skills = discover_skills(dir.path(), false);
    assert_eq!(skills.len(), 1, "{skills:?}");
    assert_eq!(skills[0].folder_path, ".gemini/skills/banner");
}

#[test]
fn full_depth_skips_test_fixture_trees() {
    let fixture = crate::pack_fixture::impeccable_like();
    let skills = discover_skills_without_dedup(fixture.dir.path(), true, None);
    let folders: Vec<&str> = skills
        .iter()
        .map(|skill| skill.folder_path.as_str())
        .collect();
    for fixture_dir in crate::pack_fixture::TEST_FIXTURES {
        assert!(
            !folders.contains(fixture_dir),
            "{fixture_dir} leaked: {folders:?}"
        );
    }
    for published in crate::pack_fixture::published_copies() {
        assert!(folders.contains(&published.as_str()), "{published} missing");
    }
}

#[test]
fn skill_named_test_inside_container_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(&dir.path().join("skills/test/SKILL.md"), "test", "qa").unwrap();
    write_skill_md(
        &dir.path().join("skills/test/fixtures/inner/SKILL.md"),
        "inner",
        "fixture",
    )
    .unwrap();

    for full_depth in [false, true] {
        let skills = discover_skills(dir.path(), full_depth);
        let folders: Vec<&str> = skills
            .iter()
            .map(|skill| skill.folder_path.as_str())
            .collect();
        assert_eq!(folders, vec!["skills/test"], "full_depth={full_depth}");
    }
}

#[test]
fn dedupe_keeps_higher_priority() {
    let skills = vec![
        DiscoveredSkill {
            id: "my-skill".to_string(),
            folder_path: ".claude/skills/my-skill".to_string(),
            description: "low priority".to_string(),
            already_installed: false,
            installable: true,
            frontmatter_issues: Vec::new(),
            internal: false,
        },
        DiscoveredSkill {
            id: "my-skill".to_string(),
            folder_path: "source/skills/my-skill".to_string(),
            description: "high priority".to_string(),
            already_installed: false,
            installable: true,
            frontmatter_issues: Vec::new(),
            internal: false,
        },
    ];
    let deduped = dedupe_discovered_skills(skills, &[]);
    assert_eq!(deduped.len(), 1);
    // Priority-container copy wins (vercel first-seen order).
    assert_eq!(deduped[0].folder_path, ".claude/skills/my-skill");
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
fn priority_skill_dirs_cover_every_builtin_agent_and_legacy_dir() {
    let mut seen = std::collections::HashSet::new();
    for dir in PRIORITY_SKILL_DIRS {
        assert!(seen.insert(*dir), "duplicate PRIORITY_SKILL_DIRS entry: {dir}");
    }

    for profile in crate::agents::list_profiles() {
        if profile.project_skills_rel.is_empty() || profile.project_skills_rel == "skills" {
            continue;
        }
        assert!(
            seen.contains(profile.project_skills_rel.as_str()),
            "PRIORITY_SKILL_DIRS missing project_skills_rel '{}' for builtin agent '{}'",
            profile.project_skills_rel,
            profile.id
        );
    }

    for legacy in [
        ".agent/skills",
        ".cline/skills",
        ".codex/skills",
        ".factory/skills",
        ".fx/skills",
        ".github/skills",
        ".kilo/skills",
        ".kilocode/skills",
        ".opencode/skills",
        ".pi/skills",
    ] {
        assert!(
            seen.contains(legacy),
            "PRIORITY_SKILL_DIRS missing legacy dir '{legacy}'"
        );
    }
}

#[test]
fn registered_agent_dirs_are_discovered_alongside_skills_dir() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();

    write_skill_md(&repo.join("skills/foo/SKILL.md"), "foo", "standard").unwrap();
    write_skill_md(&repo.join("agent/skills/bar/SKILL.md"), "bar", "eve").unwrap();
    write_skill_md(&repo.join("data/skills/baz/SKILL.md"), "baz", "astrbot").unwrap();

    let skills = discover_skills(repo, false);
    let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, vec!["bar", "baz", "foo"]);
}
