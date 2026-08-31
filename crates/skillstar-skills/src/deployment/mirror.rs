//! Replay an Agent's global deployments into its mirror directories.
//!
//! Antigravity installs as three independent states (app / CLI / IDE) under
//! `~/.gemini`, each reading skills only from its own `builtin/skills`. One
//! SkillStar profile serves all three: its `global_skills_dir` stays the single
//! bookkeeping source of truth (link counts, deploy status, unlink-all) and this
//! module reconciles every mirror against it after each deploy or unlink.
//!
//! Reconcile rather than mirror each operation: rerunning repairs states that
//! were installed later, or whose `builtin/` an Antigravity upgrade re-extracted.
//!
//! Mirrors also hold Antigravity's own bundled skills, which are real
//! directories. Only links are ever removed from a mirror, and an existing real
//! directory is never replaced.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::warn;

use skillstar_core::infra::fs_ops;

/// Reconcile every mirror directory of `agent_id` against `source_dir`.
///
/// Best-effort: mirrors are an extra deployment target, so a failure is logged
/// and never fails the caller's deploy.
pub(super) fn sync(agent_id: &str, source_dir: &Path) {
    let mirrors = skillstar_agents::global_mirror_dirs(agent_id);
    if mirrors.is_empty() {
        return;
    }

    let wanted = managed_deployments(source_dir);
    for mirror in mirrors {
        // `builtin/` is created by Antigravity itself; its absence means that
        // state is not installed and we must not conjure a skills dir for it.
        if !mirror.parent().is_some_and(Path::exists) || mirror == source_dir {
            continue;
        }
        if let Err(err) = sync_one(&mirror, &wanted) {
            warn!(
                target: "sync",
                agent = %agent_id,
                mirror = %mirror.display(),
                error = %err,
                "Failed to mirror skill deployments"
            );
        }
    }
}

/// Managed deployments in `dir` as `name -> resolved source`.
///
/// Links resolve to the hub skill they point at, so mirrors link straight to
/// the hub instead of chaining through another Agent directory. Broken links
/// resolve to nothing and are skipped.
fn managed_deployments(dir: &Path) -> BTreeMap<String, PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let managed =
                fs_ops::is_link(&path) || (path.is_dir() && path.join("SKILL.md").exists());
            if !managed {
                return None;
            }
            let source = std::fs::canonicalize(&path).ok()?;
            Some((entry.file_name().to_string_lossy().into_owned(), source))
        })
        .collect()
}

fn sync_one(mirror: &Path, wanted: &BTreeMap<String, PathBuf>) -> Result<()> {
    if !wanted.is_empty() {
        std::fs::create_dir_all(mirror)?;
    }

    for (name, source) in wanted {
        let target = mirror.join(name);
        if fs_ops::is_link(&target) {
            if std::fs::canonicalize(&target).ok().as_deref() == Some(source.as_path()) {
                continue;
            }
            fs_ops::remove_link_or_copy(&target)?;
        } else if target.exists() {
            warn!(
                target: "sync",
                path = %target.display(),
                "Mirror entry is a real directory (bundled Agent skill?) — leaving it alone"
            );
            continue;
        }
        fs_ops::create_symlink_or_copy(source, &target)?;
    }

    // Drop deployments that are gone from the source. Links only: the bundled
    // skills living beside them are real directories and must survive.
    // ponytail: the Windows copy fallback leaves mirror copies behind on
    // unlink; track deployment provenance if that ever shows up in practice.
    let Ok(entries) = std::fs::read_dir(mirror) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !wanted.contains_key(&name) && fs_ops::is_link(&path) {
            fs_ops::remove_link_or_copy(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mirror gains the source's links, keeps bundled real directories, and
    /// loses links whose source deployment is gone.
    #[test]
    fn reconciles_links_without_touching_bundled_directories() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let hub = temp.path().join("hub");
        let source = temp.path().join("source");
        let mirror = temp.path().join("state/builtin/skills");
        std::fs::create_dir_all(hub.join("alpha"))?;
        std::fs::create_dir_all(&source)?;
        std::fs::create_dir_all(temp.path().join("state/builtin"))?;
        fs_ops::create_symlink_or_copy(&hub.join("alpha"), &source.join("alpha"))?;

        // A bundled skill and a stale deployment already sit in the mirror.
        std::fs::create_dir_all(mirror.join("bundled"))?;
        std::fs::write(mirror.join("bundled/SKILL.md"), "---\nname: bundled\n---\n")?;
        std::fs::create_dir_all(hub.join("stale"))?;
        fs_ops::create_symlink_or_copy(&hub.join("stale"), &mirror.join("stale"))?;

        sync_one(&mirror, &managed_deployments(&source))?;

        assert!(fs_ops::is_link(&mirror.join("alpha")));
        assert_eq!(
            std::fs::canonicalize(mirror.join("alpha"))?,
            std::fs::canonicalize(hub.join("alpha"))?
        );
        assert!(mirror.join("bundled/SKILL.md").exists());
        assert!(!mirror.join("stale").exists());
        Ok(())
    }
}
