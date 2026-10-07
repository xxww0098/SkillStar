use super::*;
use crate::test_sandbox::Sandbox;
use std::collections::BTreeMap;

fn make_skill(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
}

/// A mirror gains the source's links, keeps bundled real directories and the
/// user's own links, and loses SkillStar links whose source deployment is gone.
#[test]
fn reconciles_links_without_touching_bundled_or_user_entries() -> Result<()> {
    let sandbox = Sandbox::new();
    let hub = paths::hub_skills_dir();
    let source = sandbox.root().join("source");
    let mirror = sandbox.root().join("state/builtin/skills");
    make_skill(&hub.join("alpha"));
    make_skill(&hub.join("stale"));
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir_all(&mirror)?;
    fs_ops::create_symlink(&hub.join("alpha"), &source.join("alpha"))?;

    make_skill(&mirror.join("bundled"));
    fs_ops::create_symlink(&hub.join("stale"), &mirror.join("stale"))?;
    let user_checkout = sandbox.root().join("dev/mine");
    make_skill(&user_checkout);
    fs_ops::create_symlink(&user_checkout, &mirror.join("mine"))?;

    let changes = sync_one(&mirror, &managed_deployments(&source))?;
    assert_eq!(changes.created, 1, "alpha must be created");
    assert_eq!(changes.removed, 1, "stale must be removed");

    assert_eq!(
        std::fs::canonicalize(mirror.join("alpha"))?,
        std::fs::canonicalize(hub.join("alpha"))?
    );
    assert!(mirror.join("bundled/SKILL.md").exists());
    assert!(mirror.join("stale").symlink_metadata().is_err());
    assert!(
        fs_ops::is_link(&mirror.join("mine")),
        "user link must survive"
    );
    Ok(())
}

/// Legacy builtin symlinks deployed by earlier versions of SkillStar are
/// cleaned up without touching bundled real directories.
#[test]
fn cleans_legacy_builtin_symlinks_without_touching_real_dirs() -> Result<()> {
    let sandbox = Sandbox::new();
    let home = sandbox.home();
    let legacy_dir = home.join(".gemini/antigravity/builtin/skills");
    std::fs::create_dir_all(&legacy_dir)?;
    let hub = paths::hub_skills_dir();
    make_skill(&hub.join("test-skill"));
    fs_ops::create_symlink(&hub.join("test-skill"), &legacy_dir.join("test-skill"))?;
    let bundled = legacy_dir.join("agy-customizations");
    make_skill(&bundled);

    cleanup_legacy_builtin_mirrors("antigravity", &home);

    assert!(legacy_dir.join("test-skill").symlink_metadata().is_err());
    assert!(bundled.join("SKILL.md").exists());
    Ok(())
}

/// Production layout: `~/.gemini/skills` is both an Antigravity mirror and
/// gemini-cli's Global directory. Reconciling Antigravity must neither add to
/// nor prune gemini-cli's own deployments.
#[test]
fn antigravity_reconcile_leaves_gemini_cli_directory_alone() -> Result<()> {
    let sandbox = Sandbox::production();
    let home = sandbox.home();
    let canonical = paths::agents_skills_root();
    assert_eq!(canonical, home.join(".skillstar/data/skills/installed"));
    make_skill(&canonical.join("shared"));
    make_skill(&canonical.join("gemini-only"));

    let antigravity = home.join(".gemini/antigravity/skills");
    std::fs::create_dir_all(&antigravity)?;
    fs_ops::create_symlink(&canonical.join("shared"), &antigravity.join("shared"))?;
    let gemini = home.join(".gemini/skills");
    std::fs::create_dir_all(&gemini)?;
    fs_ops::create_symlink(&canonical.join("gemini-only"), &gemini.join("gemini-only"))?;
    let cli_mirror = home.join(".gemini/antigravity-cli/skills");
    std::fs::create_dir_all(cli_mirror.parent().unwrap())?;

    sync("antigravity", &antigravity);

    assert!(
        fs_ops::is_link(&gemini.join("gemini-only")),
        "gemini-cli's own deployment must not be pruned"
    );
    assert!(
        gemini.join("shared").symlink_metadata().is_err(),
        "Antigravity must not deploy into gemini-cli's directory"
    );
    assert!(
        fs_ops::is_link(&cli_mirror.join("shared")),
        "real Antigravity mirrors still follow"
    );
    Ok(())
}

/// Replacing a mirror link builds the new link under a temporary name and
/// renames it into place, so the mirror is not emptied first.
#[test]
fn sync_one_replaces_a_link_without_leaving_a_staging_name() -> Result<()> {
    let sandbox = Sandbox::new();
    let canonical = paths::hub_skills_dir().join("alpha");
    let local = paths::local_skills_dir().join("alpha");
    make_skill(&canonical);
    make_skill(&local);
    let mirror = sandbox.root().join("mirror");
    std::fs::create_dir_all(&mirror)?;
    fs_ops::create_symlink(&local, &mirror.join("alpha"))?;

    let mut wanted = BTreeMap::new();
    wanted.insert("alpha".into(), std::fs::canonicalize(&canonical)?);
    sync_one(&mirror, &wanted)?;

    assert_eq!(
        std::fs::canonicalize(mirror.join("alpha"))?,
        std::fs::canonicalize(&canonical)?
    );
    let leftovers: Vec<_> = std::fs::read_dir(&mirror)?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.file_name())
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn sync_one_keeps_the_previous_link_when_the_replacement_cannot_be_staged() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new();
    let canonical = paths::hub_skills_dir().join("alpha");
    let local = paths::local_skills_dir().join("alpha");
    make_skill(&canonical);
    make_skill(&local);
    let mirror = sandbox.root().join("mirror");
    std::fs::create_dir_all(&mirror)?;
    fs_ops::create_symlink(&local, &mirror.join("alpha"))?;

    std::fs::set_permissions(&mirror, std::fs::Permissions::from_mode(0o555))?;
    let mut wanted = BTreeMap::new();
    wanted.insert("alpha".into(), std::fs::canonicalize(&canonical)?);
    let error = sync_one(&mirror, &wanted);
    std::fs::set_permissions(&mirror, std::fs::Permissions::from_mode(0o755))?;

    assert!(error.is_err());
    assert_eq!(
        std::fs::canonicalize(mirror.join("alpha"))?,
        std::fs::canonicalize(&local)?
    );
    Ok(())
}
