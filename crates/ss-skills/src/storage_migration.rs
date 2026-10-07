//! One-time v3 storage-layout migration for the skills domain (D-087).
//!
//! Moves user-authored local skills from the legacy hub
//! ('~/.skillstar/hub/local', pre-D-087) to 'data/skills/local/' and
//! re-points every agent link that exposed them. Idempotent: entries move
//! only when the target is missing, and the whole migration is skipped while
//! path overrides are active — dev/test sandboxes must never touch another
//! root's data (same guard as legacy_cleanup).

use std::path::{Path, PathBuf};

use ss_core::infra::fs_ops;
use ss_core::infra::paths;

fn overrides_active() -> bool {
    ["SKILLSTAR_HUB_DIR", "SKILLSTAR_DATA_DIR"]
        .iter()
        .any(|key| {
            std::env::var(key)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
}

/// Run once from every process entry point, after
/// ss_core::infra::migration::migrate_legacy_paths and before
/// legacy_cleanup::run_once (which must never see live links into
/// hub/local).
pub fn migrate_local_skills() {
    if overrides_active() {
        return;
    }
    let old_dir = paths::legacy_hub_root().join("local");
    let new_dir = paths::local_skills_dir();
    let old_canonical = std::fs::canonicalize(&old_dir).ok();

    let moved = if old_dir.is_dir() {
        move_entries(&old_dir, &new_dir)
    } else {
        0
    };
    // Every run, not only the one that moved content: a crash between the move
    // and the relink would otherwise leave dangling links forever.
    relink_agent_exposures(&old_dir, old_canonical.as_deref(), &new_dir);
    if dir_is_empty(&old_dir) {
        let _ = fs_ops::remove_dir_all_retry(&old_dir);
        // The legacy hub root itself may now be empty — drop it too. When
        // pre-D-081 skills/ or repos/ still exist, legacy_cleanup owns them
        // and the hub is not empty, so it stays.
        if let Some(parent) = old_dir.parent()
            && dir_is_empty(parent)
        {
            let _ = fs_ops::remove_dir_all_retry(parent);
        }
    }
    if moved > 0 {
        tracing::info!(
            target: "storage_migration",
            moved,
            "migrated local skills to data/skills/local (v3)"
        );
    }
}

/// Move installed skills out of `~/.agents/skills` into the SkillStar data root.
///
/// D-100. Idempotent and skipped while path overrides are active, same as
/// [`migrate_local_skills`]. A name that already exists at the new root stays
/// put, and the old copy is left behind. Agent links that still point at the
/// old directory are retargeted when the skill actually moved.
pub fn migrate_installed_skills() {
    if overrides_active() {
        return;
    }
    let old_dir = paths::legacy_agents_skills_root();
    let new_dir = paths::agents_skills_root();
    if old_dir == new_dir {
        return;
    }
    let old_canonical = std::fs::canonicalize(&old_dir).ok();
    let moved = if old_dir.is_dir() {
        move_entries(&old_dir, &new_dir)
    } else {
        0
    };
    let lock_moved = move_legacy_lock();
    relink_agent_exposures(&old_dir, old_canonical.as_deref(), &new_dir);
    if dir_is_empty(&old_dir) {
        let _ = std::fs::remove_dir(&old_dir);
        if let Some(parent) = old_dir.parent()
            && dir_is_empty(parent)
        {
            let _ = std::fs::remove_dir(parent);
        }
    }
    if moved > 0 || lock_moved {
        tracing::info!(
            target: "storage_migration",
            moved,
            lock_moved,
            "migrated installed skills out of ~/.agents/skills"
        );
    }
}

/// Move the lock SkillStar used to share with `~/.agents` when the new lock
/// is not already there. The first existing legacy path wins.
fn move_legacy_lock() -> bool {
    let new_lock = paths::skill_lock_path();
    if new_lock.symlink_metadata().is_ok() {
        return false;
    }
    for old in paths::legacy_skill_lock_paths() {
        if old.symlink_metadata().is_err() {
            continue;
        }
        if let Some(parent) = new_lock.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(&old, &new_lock) {
            Ok(()) => return true,
            Err(error) => tracing::warn!(
                target: "storage_migration",
                from = %old.display(),
                to = %new_lock.display(),
                %error,
                "failed to move install lock"
            ),
        }
    }
    false
}

fn dir_is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut entries| entries.next().is_none())
        .unwrap_or(false)
}

/// Move every entry of old_dir into new_dir; an existing target wins and the
/// source entry is left untouched. Returns how many entries moved.
fn move_entries(old_dir: &Path, new_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(old_dir) else {
        return 0;
    };
    let _ = std::fs::create_dir_all(new_dir);
    let mut moved = 0usize;
    for entry in entries.flatten() {
        let destination = new_dir.join(entry.file_name());
        if destination.symlink_metadata().is_ok() {
            continue;
        }
        match std::fs::rename(entry.path(), &destination) {
            Ok(()) => moved += 1,
            Err(error) => tracing::warn!(
                target: "storage_migration",
                from = %entry.path().display(),
                to = %destination.display(),
                %error,
                "failed to move local skill"
            ),
        }
    }
    moved
}

/// Recreate agent-skill links whose (possibly relative) target points into
/// the old local directory so they expose the moved content at its new home.
/// Both the lexical old path and its canonicalized form are accepted: on
/// macOS a tempdir under /var canonicalizes to /private/var, and relative
/// link text resolves lexically against the un-canonicalized prefix.
fn relink_agent_exposures(old_dir: &Path, old_canonical: Option<&Path>, new_dir: &Path) {
    // The canonical root exposes local skills through links too.
    let mut dirs = vec![paths::hub_skills_dir()];
    dirs.extend(
        crate::agents::list_profiles()
            .into_iter()
            .filter(|profile| profile.has_global_skills())
            .map(|profile| profile.global_skills_dir),
    );
    dirs.dedup();
    for dir in dirs {
        relink_in_dir(&dir, old_dir, old_canonical, new_dir);
    }
}

/// One agent global-skills directory: re-point links whose (possibly
/// relative) target resolves into the old local directory.
fn relink_in_dir(dir: &Path, old_dir: &Path, old_canonical: Option<&Path>, new_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let link = entry.path();
        if !fs_ops::is_link(&link) {
            continue;
        }
        let Some(target) = absolute_link_target(&link) else {
            continue;
        };
        let normalized = lexical_normalize(&target);
        let relative = normalized.strip_prefix(old_dir).ok().or_else(|| {
            old_canonical.and_then(|canonical| normalized.strip_prefix(canonical).ok())
        });
        let Some(relative) = relative.filter(|path| !path.as_os_str().is_empty()) else {
            continue;
        };
        // A retained top-level entry means a conflict or failed move, even if
        // this particular nested target is missing. Never switch its content.
        let Some(skill) = relative.components().next() else {
            continue;
        };
        if !matches!(
            old_dir.join(skill.as_os_str()).symlink_metadata(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ) {
            continue;
        }
        let new_target = new_dir.join(relative);
        if !new_target.is_dir() {
            continue;
        }
        if let Err(error) = replace_link(&link, &new_target) {
            tracing::warn!(
                target: "storage_migration",
                link = %link.display(),
                %error,
                "failed to re-point local skill link"
            );
        }
    }
}

/// Point `link` at `target` without a window where it is missing: build the
/// new link beside it, then rename it over the old one. On Unix the new link
/// is relative, so moving the whole home keeps it valid.
fn replace_link(link: &Path, target: &Path) -> anyhow::Result<()> {
    let parent = link
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", link.display()))?;
    let name = link.file_name().unwrap_or_default().to_string_lossy();
    let temp = parent.join(format!(
        ".skillstar-relink-{name}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs_ops::create_symlink(&link_text(parent, target), &temp)?;
    #[cfg(windows)]
    let _ = fs_ops::remove_symlink(link);
    if let Err(error) = std::fs::rename(&temp, link) {
        let _ = fs_ops::remove_symlink(&temp);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(unix)]
fn link_text(link_dir: &Path, target: &Path) -> PathBuf {
    let from = lexical_normalize(link_dir);
    let to = lexical_normalize(target);
    let common = from
        .components()
        .zip(to.components())
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return target.to_path_buf();
    }
    let mut relative = PathBuf::new();
    for _ in from.components().skip(common) {
        relative.push("..");
    }
    for component in to.components().skip(common) {
        relative.push(component.as_os_str());
    }
    relative
}

#[cfg(not(unix))]
fn link_text(_link_dir: &Path, target: &Path) -> PathBuf {
    target.to_path_buf()
}

/// Absolute target of a symlink, resolving relative link text against the
/// link's own directory (without touching the filesystem, so moved-away
/// targets still resolve lexically).
fn absolute_link_target(link: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_link(link).ok()?;
    if raw.is_absolute() {
        return Some(raw);
    }
    link.parent().map(|parent| parent.join(raw))
}

/// Lexically normalize '..' and '.' segments so prefix comparison against a
/// canonicalized directory works.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_local_skills_is_skipped_under_overrides() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        assert!(overrides_active());
        // Safe by construction: the guard returns before touching anything.
        migrate_local_skills();
    }

    #[cfg(unix)]
    #[test]
    fn installed_skills_leave_the_shared_agents_directory() {
        let sandbox = crate::test_sandbox::Sandbox::production();
        let old_skill = sandbox.home().join(".agents/skills/alpha");
        std::fs::create_dir_all(&old_skill).unwrap();
        std::fs::write(old_skill.join("SKILL.md"), "---\nname: alpha\n---\n").unwrap();
        let old_lock = sandbox.home().join(".agents/.skill-lock.json");
        std::fs::write(&old_lock, b"{}\n").unwrap();
        let claude = sandbox.home().join(".claude/skills");
        std::fs::create_dir_all(&claude).unwrap();
        ss_core::infra::fs_ops::create_symlink(
            &sandbox.home().join(".agents/skills/alpha"),
            &claude.join("alpha"),
        )
        .unwrap();

        migrate_installed_skills();
        migrate_installed_skills();

        let installed = paths::agents_skills_root().join("alpha/SKILL.md");
        assert!(installed.is_file(), "{}", installed.display());
        assert!(!old_skill.exists());
        assert!(paths::skill_lock_path().is_file());
        assert!(!old_lock.exists());
        assert_eq!(
            std::fs::canonicalize(claude.join("alpha")).unwrap(),
            std::fs::canonicalize(paths::agents_skills_root().join("alpha")).unwrap()
        );
        assert!(
            !sandbox.home().join(".agents/skills").exists(),
            "the shared agents directory must not keep the moved skill"
        );
    }

    #[cfg(unix)]
    #[test]
    fn move_entries_moves_and_target_wins() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        std::fs::create_dir_all(old.join("alpha")).unwrap();
        std::fs::create_dir_all(new.join("beta")).unwrap();
        std::fs::create_dir_all(old.join("beta")).unwrap(); // name clash

        let moved = move_entries(&old, &new);

        assert_eq!(moved, 1, "only the clash-free entry moves");
        assert!(new.join("alpha").is_dir());
        assert!(new.join("beta").is_dir(), "pre-existing target kept");
        assert!(old.join("beta").is_dir(), "clashing source left behind");
        assert!(!old.join("alpha").exists());
    }

    #[cfg(unix)]
    #[test]
    fn lexical_normalize_collapses_relative_segments() {
        let normalized = lexical_normalize(Path::new("/a/b/c/../../d/./e"));
        assert_eq!(normalized, PathBuf::from("/a/d/e"));
        let link = tempfile::tempdir().unwrap().path().join("y/z/link");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(Path::new("../../real"), &link).unwrap();
        let target = absolute_link_target(&link).unwrap();
        assert_eq!(
            lexical_normalize(&target),
            link.parent()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("real")
        );
    }

    #[cfg(unix)]
    #[test]
    fn relink_repoints_links_into_old_dir() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("hub/local");
        let new = temp.path().join("data/skills/local");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(new.join("demo")).unwrap();
        // One agent global skills dir with a relative link into old, plus an
        // unrelated link that must survive untouched.
        let agent_dir = temp.path().join("agent-skills");
        std::fs::create_dir_all(&agent_dir).unwrap();
        std::os::unix::fs::symlink(Path::new("../hub/local/demo"), agent_dir.join("demo")).unwrap();
        std::fs::create_dir_all(temp.path().join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("elsewhere"), agent_dir.join("other")).unwrap();

        let old_canonical = std::fs::canonicalize(&old).unwrap();
        relink_in_dir(&agent_dir, &old, Some(&old_canonical), &new);

        let relinked = std::fs::read_link(agent_dir.join("demo")).unwrap();
        assert_eq!(relinked, Path::new("../data/skills/local/demo"));
        assert!(
            std::fs::canonicalize(agent_dir.join("demo"))
                .unwrap()
                .is_dir()
        );
        assert_eq!(
            std::fs::read_link(agent_dir.join("other")).unwrap(),
            temp.path().join("elsewhere")
        );
    }

    #[cfg(unix)]
    #[test]
    fn relink_preserves_conflicts_and_maps_actual_target_paths() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        let agents = temp.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::create_dir_all(old.join("conflict")).unwrap();
        std::fs::create_dir_all(new.join("conflict/nested")).unwrap();
        std::fs::create_dir_all(new.join("moved/nested")).unwrap();
        std::fs::create_dir_all(new.join("dangling")).unwrap();
        std::os::unix::fs::symlink("missing", old.join("dangling")).unwrap();
        for (name, target) in [
            ("conflict", "../old/conflict"),
            ("nested-conflict", "../old/conflict/nested"),
            ("dangling", "../old/dangling"),
            ("alias", "../old/moved"),
            ("nested-alias", "../old/moved/nested"),
        ] {
            std::os::unix::fs::symlink(target, agents.join(name)).unwrap();
        }

        for _ in 0..2 {
            relink_in_dir(&agents, &old, None, &new);
            for (name, target) in [
                ("conflict", "../old/conflict"),
                ("nested-conflict", "../old/conflict/nested"),
                ("dangling", "../old/dangling"),
                ("alias", "../new/moved"),
                ("nested-alias", "../new/moved/nested"),
            ] {
                assert_eq!(
                    std::fs::read_link(agents.join(name)).unwrap(),
                    Path::new(target)
                );
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn relink_runs_again_after_an_interrupted_migration() {
        let sandbox = crate::test_sandbox::Sandbox::production();
        let new = paths::local_skills_dir();
        std::fs::create_dir_all(new.join("demo")).unwrap();
        std::fs::write(new.join("demo/SKILL.md"), "---\nname: demo\n---\n").unwrap();
        // The move already happened and the old directory is gone, but the
        // canonical link still points at it.
        let old = paths::legacy_hub_root().join("local/demo");
        let canonical = paths::hub_skills_dir();
        std::fs::create_dir_all(&canonical).unwrap();
        std::os::unix::fs::symlink(&old, canonical.join("demo")).unwrap();

        migrate_local_skills();
        migrate_local_skills();

        let text = std::fs::read_link(canonical.join("demo")).unwrap();
        assert!(text.is_relative(), "{}", text.display());
        assert!(canonical.join("demo/SKILL.md").is_file());
        let leftovers = std::fs::read_dir(&canonical)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".skillstar-")
            })
            .count();
        assert_eq!(leftovers, 0);
        drop(sandbox);
    }
}
