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

fn inventory(repo: &Path) -> Inventory {
    load_or_plan(repo, &GitOperationSession::public()).expect("plan succeeds")
}

fn sparse(repo: &Path, extra: &[String]) -> Vec<String> {
    inventory(repo).sparse_dirs(extra, &[])
}

/// Treeless sparse clone of `remote`, as a real cache entry starts.
fn partial_clone(remote: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::git::ops::clone_repo_sparse_in_session(
        &crate::git::ops::local_file_url(remote),
        &dir.path().join("repo"),
        &GitOperationSession::public(),
    )
    .expect("partial clone");
    dir
}

fn promisor_packs(repo: &Path) -> usize {
    std::fs::read_dir(repo.join(".git/objects/pack"))
        .unwrap()
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "promisor")
        })
        .count()
}

/// The impeccable shape: one skill mirrored into many harness directories
/// with identical content, plus a canonical `.agents/skills` copy.
#[test]
fn harness_copies_collapse_to_one_representative() {
    let repo = tempfile::tempdir().unwrap();
    for harness in [".claude", ".cursor", ".codex", ".agent"] {
        write(
            &repo
                .path()
                .join(format!("{harness}/skills/impeccable/SKILL.md")),
            "---\nname: impeccable\ndescription: d\n---\nbody\n",
        );
    }
    write(
        &repo.path().join(".agents/skills/impeccable/SKILL.md"),
        "---\nname: impeccable\ndescription: d\n---\nbody\n",
    );
    // Heavy non-skill content that must never influence the plan.
    write(
        &repo.path().join("crates/engine/src/lib.rs"),
        "pub fn f() {}",
    );
    write(&repo.path().join("README.md"), "# pack");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = inventory(repo.path());
    assert_eq!(
        plan.sparse_dirs(&[], &[]),
        vec![".agents/skills/impeccable".to_string()]
    );
    assert_eq!(
        plan.deferred(),
        vec![
            ".agent/skills/impeccable".to_string(),
            ".claude/skills/impeccable".to_string(),
            ".codex/skills/impeccable".to_string(),
            ".cursor/skills/impeccable".to_string(),
        ]
    );
    // Sidecar persisted and reloads identically.
    assert!(repo.path().join(SIDECAR).is_file());
    assert_eq!(inventory(repo.path()), plan);
}

/// Same basename, different frontmatter `name`: two Skills, both materialize.
#[test]
fn same_basename_different_name_materializes_both() {
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

    let plan = inventory(repo.path());
    assert_eq!(
        plan.sparse_dirs(&[], &[]),
        vec![
            "demos/impeccable".to_string(),
            "skills/impeccable".to_string(),
        ]
    );
    assert!(plan.deferred().is_empty());
    assert!(plan.identities.contains_key("impeccable-demo"));
}

/// Harness-rewritten copies (same `name`, different bytes) are one identity:
/// only the representative materializes.
#[test]
fn divergent_same_name_copy_defers() {
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

    let plan = inventory(repo.path());
    assert_eq!(
        plan.sparse_dirs(&[], &[]),
        vec!["skills/impeccable".to_string()]
    );
    assert_eq!(
        plan.deferred(),
        vec![".cursor/skills/impeccable".to_string()]
    );
}

/// Identical-content copies (same tree SHA) defer without reading any name.
#[test]
fn identical_harness_copy_defers_but_is_recorded() {
    let repo = tempfile::tempdir().unwrap();
    let body = "---\nname: impeccable\ndescription: d\n---\nsame bytes\n";
    write(&repo.path().join("skills/impeccable/SKILL.md"), body);
    write(
        &repo.path().join(".cursor/skills/impeccable/SKILL.md"),
        body,
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = inventory(repo.path());
    assert_eq!(
        plan.sparse_dirs(&[], &[]),
        vec!["skills/impeccable".to_string()]
    );
    assert_eq!(
        plan.deferred(),
        vec![".cursor/skills/impeccable".to_string()]
    );
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

    let dirs = sparse(repo.path(), &[".cursor/skills/impeccable".to_string()]);
    assert!(dirs.contains(&".cursor/skills/impeccable".to_string()));
}

/// One ranking table (`pack_layout::choose_copy`): catalog, then the shared
/// `.agents/skills`, then manifest containers. Identical bytes make the
/// losers deferrable, so the representative is observable.
#[test]
fn canonical_and_agents_beat_manifest_container() {
    let body = "---\nname: impeccable\ndescription: d\n---\nsame\n";
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join(".claude-plugin/marketplace.json"),
        r#"{ "plugins": [ { "name": "p", "source": "./plugin" } ] }"#,
    );
    write(&repo.path().join("plugin/skills/impeccable/SKILL.md"), body);
    write(
        &repo.path().join(".agents/skills/impeccable/SKILL.md"),
        body,
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());
    let plan_agents = inventory(repo.path());
    assert_eq!(
        plan_agents.sparse_dirs(&[], &[]),
        vec![
            ".agents/skills/impeccable".to_string(),
            ".claude-plugin".to_string()
        ]
    );

    write(&repo.path().join("skills/impeccable/SKILL.md"), body);
    commit_all(repo.path());
    let plan_catalog = inventory(repo.path());
    assert_eq!(
        plan_catalog.sparse_dirs(&[], &[]),
        vec![
            ".claude-plugin".to_string(),
            "skills/impeccable".to_string()
        ]
    );
}

/// Many skills under one parent compact into that parent — one cone pattern
/// instead of N.
#[test]
fn siblings_compact_to_parent() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("skills/alpha/SKILL.md"),
        "---\nname: alpha\n---\n",
    );
    write(
        &repo.path().join("skills/beta/SKILL.md"),
        "---\nname: beta\n---\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = inventory(repo.path());
    assert_eq!(plan.sparse_dirs(&[], &[]), vec!["skills".to_string()]);
}

/// A repo with no nested SKILL.md keeps the legacy behavior: empty plan, the
/// caller disables sparse and checks out everything.
#[test]
fn no_nested_skills_yields_empty_plan() {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("README.md"), "# plain repo");
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let plan = inventory(repo.path());
    assert!(plan.is_unfiltered());
    assert!(plan.sparse_dirs(&[], &[]).is_empty());
}

/// Moving HEAD invalidates the sidecar and forces a rebuild.
#[test]
fn revision_change_rebuilds_plan() {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("skills/alpha/SKILL.md"),
        "---\nname: alpha\n---\n",
    );
    git(repo.path(), &["init", "-q"]);
    commit_all(repo.path());

    let first = inventory(repo.path());
    write(
        &repo.path().join("skills/beta/SKILL.md"),
        "---\nname: beta\n---\n",
    );
    commit_all(repo.path());

    let second = inventory(repo.path());
    assert_eq!(second.sparse_dirs(&[], &[]), vec!["skills".to_string()]);
    assert_ne!(first.revision, second.revision);
}

/// The impeccable shape end to end: one copy per identity, the decoy with a
/// different `name` kept, fixtures never planned, the plugin manifest on disk.
#[test]
fn impeccable_fixture_materializes_one_copy_per_identity() {
    let fixture = crate::pack_fixture::impeccable_like();
    let plan = inventory(fixture.dir.path());
    assert_eq!(
        plan.sparse_dirs(&[], &[]),
        vec![
            ".agents/skills/impeccable".to_string(),
            ".claude-plugin".to_string(),
            crate::pack_fixture::DECOY.to_string(),
        ]
    );
    let mut published = crate::pack_fixture::published_copies();
    published.sort();
    assert_eq!(plan.identities["impeccable"], published);
    let mut fixtures: Vec<String> = crate::pack_fixture::TEST_FIXTURES
        .iter()
        .map(|dir| dir.to_string())
        .collect();
    fixtures.sort();
    assert_eq!(plan.ignored, fixtures);
}

/// Names come from one prefetch of the missing `SKILL.md` blobs.
#[test]
fn identity_resolution_costs_one_fetch() {
    // Sandbox tests swap HOME / git config process-wide; hold the lock
    // while git talks to the promisor remote.
    let _env = crate::lock_test_env();
    let fixture = crate::pack_fixture::impeccable_like();
    let clone = partial_clone(fixture.dir.path());
    let repo = clone.path().join("repo");
    let before = promisor_packs(&repo);
    let plan = inventory(&repo);
    assert_eq!(promisor_packs(&repo), before + 1);
    assert!(
        plan.deferred()
            .contains(&".cursor/skills/impeccable".to_string())
    );
}

/// No names readable (remote gone): the group falls back to the tree-SHA
/// rule, so every byte-distinct copy stays materialized — never lost.
#[test]
fn unreadable_manifest_degrades_to_tree_sha_rule() {
    // Sandbox tests swap HOME / git config process-wide; hold the lock
    // while git talks to the promisor remote.
    let _env = crate::lock_test_env();
    let fixture = crate::pack_fixture::impeccable_like();
    let clone = partial_clone(fixture.dir.path());
    let repo = clone.path().join("repo");
    git(
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            "file:///nonexistent/skillstar-remote",
        ],
    );
    let plan = inventory(&repo);
    assert!(plan.deferred().is_empty(), "{:?}", plan.deferred());
    for copy in crate::pack_fixture::published_copies() {
        assert!(plan.materialized.contains(&copy), "{copy} lost");
    }
}

#[test]
fn compaction_never_swallows_deferred_sibling() {
    let dirs = vec!["pack/a".to_string(), "pack/b".to_string()];
    assert_eq!(
        compact_to_common_parents(&dirs, &["pack/c".to_string()]),
        dirs
    );
    assert_eq!(
        compact_to_common_parents(&dirs, &[]),
        vec!["pack".to_string()]
    );
}

#[test]
fn old_format_sidecar_is_rebuilt() {
    let fixture = crate::pack_fixture::impeccable_like();
    let repo = fixture.dir.path();
    let head = crate::pack_fixture::git(repo, &["rev-parse", "HEAD"]);
    write(
        &repo.join(SIDECAR),
        &format!(r#"{{"revision":"{head}","sparse_dirs":["x"],"deferred_dirs":[]}}"#),
    );
    let plan = inventory(repo);
    assert_eq!(plan.format, INVENTORY_FORMAT);
    assert!(plan.identities.contains_key("impeccable"));
}

/// Re-applying (after a fetch/reset) only adds: a copy another Agent is
/// pinned to stays on disk, and installed folders join even on a sidecar hit.
#[test]
fn reapply_keeps_on_disk_copies_and_adds_installed() {
    // Sandbox tests swap HOME / git config process-wide; hold the lock
    // while git talks to the promisor remote.
    let _env = crate::lock_test_env();
    let fixture = crate::pack_fixture::impeccable_like();
    let clone = partial_clone(fixture.dir.path());
    let repo = clone.path().join("repo");
    let session = GitOperationSession::public();
    apply(&repo, &session, &[]).unwrap();
    let skill = |dir: &str| repo.join(dir).join("SKILL.md").is_file();
    assert!(skill(".agents/skills/impeccable"));
    assert!(!skill(".cursor/skills/impeccable"));
    assert!(repo.join(".claude-plugin/marketplace.json").is_file());
    assert!(!repo.join("tests").exists());
    assert!(!repo.join("crates").exists());

    crate::git::ops::add_sparse_checkout_dirs_in_session(
        &repo,
        &[".cursor/skills/impeccable".to_string()],
        &session,
    )
    .unwrap();
    apply(&repo, &session, &[".dsh/skills/impeccable".to_string()]).unwrap();
    assert!(skill(".cursor/skills/impeccable"), "on-disk copy dropped");
    assert!(skill(".dsh/skills/impeccable"), "installed folder missing");
    assert!(!skill(".claude/skills/impeccable"));
}
