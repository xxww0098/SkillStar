//! A tree URL's subpath is a hard pin (slice 04): it always installs exactly
//! that copy, survives harness clicks and plain reinstalls, and (per
//! `update_checker/tests.rs::pinned_skill_update_follows_its_folder`) drives
//! updates through the pinned `source_folder` like any other install.

use super::*;
use crate::skill_install::install_skill_in_session;

const IMPECCABLE_TREE_URL: &str =
    "https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable";

fn install_pinned(url: &str) {
    install_skill_in_session(url.to_string(), None, None, &public_session())
        .unwrap_or_else(|error| panic!("pinned install of {url}: {error}"));
}

fn hub_skill_md(name: &str) -> String {
    std::fs::read_to_string(
        skillstar_core::infra::paths::hub_skills_dir()
            .join(name)
            .join("SKILL.md"),
    )
    .unwrap()
}

/// A tree URL's ref uses its own `--ref--<git_ref>` cache entry (see
/// `cache_key_for`), distinct from `impeccable_cache()`'s unpinned key.
fn impeccable_ref_cache(git_ref: &str) -> std::path::PathBuf {
    skillstar_core::infra::paths::repos_cache_dir().join(
        repo_scanner::cache_key_for("pbakaus/impeccable", Some(git_ref)).expect("valid cache key"),
    )
}

#[test]
fn tree_url_subpath_installs_exactly_that_copy() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());

    install_pinned(IMPECCABLE_TREE_URL);

    let entry = lock_entry("impeccable").expect("lock entry");
    assert_eq!(
        entry.source_folder.as_deref(),
        Some(".claude/skills/impeccable")
    );
    assert!(entry.pinned, "a tree URL subpath must pin the skill");
    assert!(
        hub_skill_md("impeccable").contains(".claude/skills/impeccable/scripts/impeccable"),
        "the hub payload must be the pinned copy's own content"
    );
}

/// Decision 5's exception: a pin can point inside an otherwise-ignored
/// fixture tree and still install.
#[test]
fn pinned_subpath_inside_tests_dir_installs() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    let pinned = "tests/oracle/workspaces/ctx-pin/.claude/skills/impeccable";
    let url = format!("https://github.com/pbakaus/impeccable/tree/main/{pinned}");

    install_pinned(&url);

    let entry = lock_entry("impeccable").expect("lock entry");
    assert_eq!(entry.source_folder.as_deref(), Some(pinned));
    assert!(entry.pinned);
}

/// Once pinned, a harness click never retargets the skill: it keeps
/// deploying the pinned folder's own content, and the lock is unchanged.
#[test]
fn pinned_skill_is_not_retargeted_by_harness_click() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    install_pinned(IMPECCABLE_TREE_URL);

    install_impeccable(Some("cursor"));

    let entry = lock_entry("impeccable").expect("lock entry");
    assert!(entry.pinned, "harness click must not clear the pin");
    assert_eq!(
        entry.source_folder.as_deref(),
        Some(".claude/skills/impeccable"),
        "harness click must not retarget a pinned skill"
    );
    assert!(
        hub_skill_md("impeccable").contains(".claude/skills/impeccable/scripts/impeccable"),
        "the deployed payload must still be the pinned copy"
    );
}

/// Only uninstalling clears a pin — reinstalling with the plain repo URL
/// must not.
#[test]
fn plain_reinstall_keeps_pin() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    install_pinned(IMPECCABLE_TREE_URL);

    install_impeccable(None);

    let entry = lock_entry("impeccable").expect("lock entry");
    assert!(entry.pinned, "a plain-URL reinstall must not clear the pin");
    assert_eq!(
        entry.source_folder.as_deref(),
        Some(".claude/skills/impeccable")
    );
}

/// 06: a `git_ref`-pinned cold clone stays sparse — it never pulls the
/// fixture's heavy non-skill content or harness copies the install never
/// asked for, only root files, the default representative copy, and the
/// pinned subpath.
#[test]
fn ref_pinned_cache_is_sparse() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());

    install_pinned(IMPECCABLE_TREE_URL);

    let cache = impeccable_ref_cache("main");
    assert!(
        cache.join(".claude/skills/impeccable/SKILL.md").is_file(),
        "the pinned copy must be on disk"
    );
    assert!(
        !cache.join("crates/engine/src/lib.rs").exists(),
        "heavy non-skill content must stay unmaterialized"
    );
    for harness in [
        ".cursor",
        ".dsh",
        ".gemini",
        ".github",
        ".grok",
        ".kiro",
        ".opencode",
    ] {
        assert!(
            !cache.join(harness).join("skills/impeccable").exists(),
            "{harness} copy must stay deferred, only the pin and the default representative materialize"
        );
    }
}

/// 06: a subpath pinned inside an otherwise-ignored fixture tree only
/// materializes that one folder, not its sibling fixture copies.
#[test]
fn tree_url_install_never_materializes_outside_subpath() {
    let sandbox = Sandbox::new();
    let fixture = crate::pack_fixture::impeccable_like();
    sandbox.map_github_url(IMPECCABLE_URL, fixture.dir.path());
    let pinned = "tests/oracle/workspaces/ctx-pin/.claude/skills/impeccable";
    let url = format!("https://github.com/pbakaus/impeccable/tree/main/{pinned}");

    install_pinned(&url);

    let cache = impeccable_ref_cache("main");
    assert!(cache.join(pinned).join("SKILL.md").is_file());
    assert!(
        !cache
            .join("tests/oracle/workspaces/ctx-pin/.cursor/skills/impeccable")
            .exists(),
        "a sibling fixture copy under the same ignored tree must stay deferred"
    );
    assert!(
        !cache
            .join("tests/oracle/workspaces/ctx-pin/.claude/skills/audit")
            .exists(),
        "an unrelated fixture Skill under the same ignored tree must stay deferred"
    );
    assert!(!cache.join("crates/engine/src/lib.rs").exists());
}
