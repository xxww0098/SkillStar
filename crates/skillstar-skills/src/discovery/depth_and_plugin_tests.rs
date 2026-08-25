use super::*;

fn write_skill_md(path: &std::path::Path, name: &str) {
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        path,
        format!("---\nname: {name}\ndescription: {name} description\n---\n\n# {name}\n"),
    )
    .unwrap();
}

#[test]
fn root_first_discovers_catalog_layouts_inside_containers() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");

    // Flat layout and a two-level catalog layout under the same container.
    write_skill_md(&repo.join("skills/flat/SKILL.md"), "flat");
    write_skill_md(
        &repo.join("skills/security/web/reviewer/SKILL.md"),
        "web-reviewer",
    );

    let skills = discover_skills(&repo, false);
    let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"flat"), "{ids:?}");
    assert!(ids.contains(&"web-reviewer"), "{ids:?}");
}

#[test]
fn a_skill_shadows_anything_nested_below_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");

    write_skill_md(&repo.join("skills/foo/SKILL.md"), "foo");
    write_skill_md(&repo.join("skills/foo/bar/SKILL.md"), "bar");

    let skills = discover_skills(&repo, false);
    let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"foo"), "{ids:?}");
    assert!(!ids.contains(&"bar"), "{ids:?}");
}

#[test]
fn full_depth_still_descends_past_shadowing_skills() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");

    write_skill_md(&repo.join("skills/foo/SKILL.md"), "foo");
    write_skill_md(&repo.join("skills/foo/bar/SKILL.md"), "bar");

    let skills = discover_skills(&repo, true);
    let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"bar"), "{ids:?}");
}

#[test]
fn plugin_manifest_declared_skills_are_discovered() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("owner--repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(repo.join(".claude-plugin")).unwrap();

    std::fs::write(
        repo.join(".claude-plugin/marketplace.json"),
        r#"{
          "metadata": { "pluginRoot": "./plugins" },
          "plugins": [
            { "name": "review", "source": "./review", "skills": ["./skills/review"] }
          ]
        }"#,
    )
    .unwrap();
    write_skill_md(
        &repo.join("plugins/review/skills/review/SKILL.md"),
        "review",
    );

    let skills = discover_skills(&repo, false);
    let ids: Vec<&str> = skills.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"review"), "{ids:?}");
}
