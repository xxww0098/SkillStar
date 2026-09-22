use super::*;

use crate::git::transport::GitOperationSession;

fn git(repo: &Path, args: &[&str]) {
    let output = skillstar_core::infra::path_env::command_with_path("git")
        .current_dir(repo)
        .args(args)
        .output()
        .expect("git spawns");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn commit_all(repo: &Path) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "fixture",
            "--no-verify",
        ],
    );
}

fn plan(repo: &Path, installed: &[String]) -> SparsePlan {
    load_or_plan(repo, &GitOperationSession::public(), installed).expect("plan succeeds")
}

/// The impeccable shape: one skill mirrored into many harness directories
/// with identical content, plus a canonical `.agents/skills` copy.
#[test]
fn harness_copies_collapse_to_one_representative() {
    let repo = tempfile::tempdir().unwrap();
    for harness in [".claude", ".cursor", ".codex", ".agent"] {
        write(
            &repo.path().join(format!("{harness}/skills/impeccable/SKILL.md")),
            "---\nname: impeccable\ndescription: d\n---\nbody\n",
        );
    }
    write(
        &repo.path().join(".agents/skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\nbody\n",
    );
    // Heavy non-skill content that must never influence the plan.
    write(&repo.path().join("crates/engine/src/lib.rs"), "pub fn f() {}");
    write(&repo.path().join("README.md"), "# pack");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert_eq!(plan.sparse_dirs, vec![".agents/skills/impeccable".to_string()]);
    assert_eq!(
        plan.deferred_dirs,
        vec![
            ".agent/skills/impeccable".to_string(),
            ".claude/skills/impeccable".to_string(),
            ".codex/skills/impeccable".to_string(),
            ".cursor/skills/impeccable".to_string(),
        ]
    );
    // Sidecar persisted and reloads identically.
    assert!(repo.path().join(SIDECAR).is_file());
    let reloaded = load_or_plan(repo.path(), &GitOperationSession::public(), &[]).unwrap();
    assert_eq!(reloaded, plan);
}

/// A copy with *different* content under a non-container parent is not a
/// duplicate: its frontmatter may declare another identity, so it stays
/// materialized.
#[test]
fn different_content_outside_containers_stays_materialized() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\n",
    );
    write(
        &repo.path().join("demos/impeccable/SKILL.md"),
        "---\nname: impeccable-demo\ndescription: demo\n---\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert_eq!(
        plan.sparse_dirs,
        vec![
            "demos/impeccable".to_string(),
            "skills/impeccable".to_string(),
        ]
    );
    assert!(plan.deferred_dirs.is_empty());
}

/// Content-divergent copies always materialize — their frontmatter may
/// declare another identity, and hiding that would lose a skill discovery
/// can find today. Only byte-identical copies defer.
#[test]
fn divergent_harness_copy_stays_materialized() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\nbase\n",
    );
    write(
        &repo.path().join(".cursor/skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\ncursor-specific\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert_eq!(
        plan.sparse_dirs,
        vec![
            ".cursor/skills/impeccable".to_string(),
            "skills/impeccable".to_string(),
        ]
    );
    assert!(plan.deferred_dirs.is_empty());
}

/// Identical-content copies (same tree SHA) defer regardless of location;
/// they come back on demand via `deferred_matches`.
#[test]
fn identical_harness_copy_defers_but_is_recorded() {
    let repo = tempfile::tempdir().unwrap();
    let body = "---\nname: impeccable\ndescription: d\n---\nsame bytes\n";
    write(&repo.path().join("skills/impeccable/SKILL.md"), body);
    write(&repo.path().join(".cursor/skills/impeccable/SKILL.md"), body);
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert_eq!(plan.sparse_dirs, vec!["skills/impeccable".to_string()]);
    assert_eq!(
        plan.deferred_dirs,
        vec![".cursor/skills/impeccable".to_string()]
    );

    let matches = plan.deferred_matches(Some("Impeccable"), None);
    assert_eq!(matches, vec![".cursor/skills/impeccable".to_string()]);
    let by_prefix = plan.deferred_matches(None, Some(".cursor"));
    assert_eq!(by_prefix.len(), 1);
    assert!(plan.deferred_matches(Some("other"), None).is_empty());
}

/// A previously installed skill's source folder must stay materialized even
/// when it is not the representative, or a fetch/reset would dangle its link.
#[test]
fn installed_source_folder_always_materializes() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\n",
    );
    write(
        &repo.path().join(".cursor/skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\ncursor\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[".cursor/skills/impeccable".to_string()]);
    assert!(plan.sparse_dirs.contains(&".cursor/skills/impeccable".to_string()));
    assert!(plan.deferred_dirs.is_empty());
}

/// Manifest-declared containers outrank the canonical `skills/` layout.
#[test]
fn manifest_declared_container_wins() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join(".claude-plugin/marketplace.json"),
        r#"{ "plugins": [ { "name": "p", "source": "./plugin" } ] }"#,
    );
    write(
        &repo.path().join("plugin/skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\nplugin\n",
    );
    write(
        &repo.path().join("skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\nplain\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    // The manifest container's copy wins; the plain copy is a different tree
    // outside a harness container, so it also materializes (same rule as the
    // demos case). The assertion that matters: the plugin copy is chosen over
    // `skills/` when only one can be the representative.
    assert!(plan.sparse_dirs.contains(&"plugin/skills/impeccable".to_string()));
}

/// Many skills under one parent compact into that parent — one cone pattern
/// instead of N.
#[test]
fn siblings_compact_to_parent() {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("skills/alpha/SKILL.md"), "---\nname: alpha\n---\n");
    write(&repo.path().join("skills/beta/SKILL.md"), "---\nname: beta\n---\n");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert_eq!(plan.sparse_dirs, vec!["skills".to_string()]);
}

/// A repo with no nested SKILL.md keeps the legacy behavior: empty plan, the
/// caller disables sparse and checks out everything.
#[test]
fn no_nested_skills_yields_empty_plan() {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("README.md"), "# plain repo");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = plan(repo.path(), &[]);
    assert!(plan.sparse_dirs.is_empty());
    assert!(plan.deferred_dirs.is_empty());
}

/// Moving HEAD invalidates the sidecar and forces a rebuild.
#[test]
fn revision_change_rebuilds_plan() {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("skills/alpha/SKILL.md"), "---\nname: alpha\n---\n");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let first = plan(repo.path(), &[]);
    write(&repo.path().join("skills/beta/SKILL.md"), "---\nname: beta\n---\n");
    commit_all(repo.path());

    let second = plan(repo.path(), &[]);
    assert_eq!(second.sparse_dirs, vec!["skills".to_string()]);
    assert_ne!(first.revision, second.revision);
}
