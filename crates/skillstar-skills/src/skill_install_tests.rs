use super::{
    RepoInstallProvenance, derive_name_hint, find_target_skill, write_repo_install_provenance,
};
use crate::frontmatter::split_front_matter;
use crate::repo_scanner::DiscoveredSkill;
use serde_yaml::Value;

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

fn write_skill_md(dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join("SKILL.md"), content).unwrap();
}

fn read_skill_md(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("SKILL.md")).unwrap()
}

#[test]
fn provenance_writer_adds_frontmatter_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(dir.path(), "# Skill\n\nBody\n");

    write_repo_install_provenance(
        dir.path(),
        RepoInstallProvenance {
            git_url: "https://github.com/example/skill-repo",
            source_folder: None,
        },
    )
    .unwrap();

    let rendered = read_skill_md(dir.path());
    let split = split_front_matter(&rendered);
    assert_eq!(split.body, "# Skill\n\nBody\n");
    assert_eq!(
        split
            .data
            .get("provenance")
            .and_then(Value::as_mapping)
            .and_then(|mapping| mapping.get(Value::String("repository_url".to_string())))
            .and_then(Value::as_str),
        Some("https://github.com/example/skill-repo")
    );
}

#[test]
fn provenance_writer_preserves_existing_frontmatter_keys_and_body() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(
        dir.path(),
        "---\ntitle: Existing\ntags:\n  - rust\n---\n# Heading\n\nOriginal body\n",
    );

    write_repo_install_provenance(
        dir.path(),
        RepoInstallProvenance {
            git_url: "https://github.com/example/skill-repo",
            source_folder: Some("skills/rust"),
        },
    )
    .unwrap();

    let rendered = read_skill_md(dir.path());
    let split = split_front_matter(&rendered);

    assert_eq!(split.body, "# Heading\n\nOriginal body\n");
    assert_eq!(
        split.data.get("title").and_then(Value::as_str),
        Some("Existing")
    );
    assert_eq!(
        split
            .data
            .get("tags")
            .and_then(Value::as_sequence)
            .and_then(|tags| tags.first())
            .and_then(Value::as_str),
        Some("rust")
    );

    let provenance = split
        .data
        .get("provenance")
        .and_then(Value::as_mapping)
        .unwrap();
    assert_eq!(
        provenance
            .get(Value::String("repository_url".to_string()))
            .and_then(Value::as_str),
        Some("https://github.com/example/skill-repo")
    );
    assert_eq!(
        provenance
            .get(Value::String("source_folder".to_string()))
            .and_then(Value::as_str),
        Some("skills/rust")
    );
}

#[test]
fn provenance_writer_merges_existing_provenance_mapping() {
    let dir = tempfile::tempdir().unwrap();
    write_skill_md(
        dir.path(),
        "---\nprovenance:\n  imported_by: skillstar\n  repository_url: stale\n---\n# Heading\n",
    );

    write_repo_install_provenance(
        dir.path(),
        RepoInstallProvenance {
            git_url: "https://github.com/example/skill-repo",
            source_folder: Some("nested/skill"),
        },
    )
    .unwrap();

    let rendered = read_skill_md(dir.path());
    let split = split_front_matter(&rendered);
    let provenance = split
        .data
        .get("provenance")
        .and_then(Value::as_mapping)
        .unwrap();

    assert_eq!(
        provenance
            .get(Value::String("imported_by".to_string()))
            .and_then(Value::as_str),
        Some("skillstar")
    );
    assert_eq!(
        provenance
            .get(Value::String("repository_url".to_string()))
            .and_then(Value::as_str),
        Some("https://github.com/example/skill-repo")
    );
    assert_eq!(
        provenance
            .get(Value::String("source_folder".to_string()))
            .and_then(Value::as_str),
        Some("nested/skill")
    );
}
