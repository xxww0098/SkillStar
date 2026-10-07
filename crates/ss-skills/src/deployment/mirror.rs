//! Replay an Agent's global deployments into its mirror directories.
//!
//! Antigravity supports multiple runtime surfaces (App / CLI / IDE / Shared) under
//! `~/.gemini`. One SkillStar profile serves all of them: its `global_skills_dir`
//! stays the single bookkeeping source of truth (link counts, deploy status, unlink-all)
//! and this module reconciles every mirror against it after each deploy or unlink.
//!
//! Reconcile rather than mirror each operation: rerunning repairs states that
//! were installed later.
//!
//! Mirrors also hold external or bundled skills that are real directories.
//! Only entries [`super::ownership`] attributes to SkillStar are ever replaced
//! or removed. A mirror that is itself some Agent's Global skills directory
//! (Antigravity's shared `~/.gemini/skills` is gemini-cli's) or the canonical
//! root is never reconciled: its contents belong to that Agent's own toggles.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use tracing::warn;

use ss_core::infra::{fs_ops, paths};

use super::ownership::{self, Ownership};
use crate::materialize;

/// Legacy mirror directories in system-managed `builtin/` spaces that previous
/// versions of SkillStar incorrectly deployed to.
const LEGACY_BUILTIN_MIRRORS: &[(&str, &[&[&str]])] = &[(
    "antigravity",
    &[
        &[".gemini", "antigravity", "builtin", "skills"],
        &[".gemini", "antigravity-cli", "builtin", "skills"],
        &[".gemini", "antigravity-ide", "builtin", "skills"],
    ],
)];

/// Clean up legacy symlinks left by previous versions of SkillStar in system `builtin/` dirs.
/// Real directories (Google-bundled built-in skills) are never touched.
fn cleanup_legacy_builtin_mirrors(agent_id: &str, home: &Path) {
    for (id, dirs) in LEGACY_BUILTIN_MIRRORS {
        if *id != agent_id {
            continue;
        }
        for parts in *dirs {
            let legacy_dir = parts
                .iter()
                .fold(home.to_path_buf(), |p, part| p.join(part));
            if !legacy_dir.exists() {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&legacy_dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if matches!(
                    ownership::owned_deployment(&path, &name),
                    Ownership::Link { .. }
                ) {
                    let _ = materialize::remove_entry(&path);
                }
            }
        }
    }
}

/// Reconcile every mirror directory of `agent_id` against `source_dir`.
///
/// Best-effort: mirrors are an extra deployment target, so a failure is logged
/// and never fails the caller's deploy.
pub(crate) fn sync(agent_id: &str, source_dir: &Path) {
    let mirrors = crate::agents::global_mirror_dirs(agent_id);
    if mirrors.is_empty() {
        return;
    }

    let home = ss_core::infra::paths::home_dir();
    cleanup_legacy_builtin_mirrors(agent_id, &home);

    let wanted = managed_deployments(source_dir);
    let reserved = reserved_dirs(source_dir);
    let mut removed = 0usize;
    let mut created = 0usize;
    for mirror in mirrors {
        // The mirror's parent must exist; its absence means that runtime
        // surface is not installed and we must not conjure a skills dir for it.
        if !mirror.parent().is_some_and(Path::exists)
            || reserved.contains(&fs_ops::canonicalize_existing_prefix(&mirror))
        {
            continue;
        }
        match sync_one(&mirror, &wanted) {
            Ok(changes) => {
                removed += changes.removed;
                created += changes.created;
            }
            Err(err) => {
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
    // INFO is reserved for user-visible outcomes: one summary line per
    // reconcile that actually changed something — never one line per path.
    if removed > 0 || created > 0 {
        tracing::info!(
            target: "sync",
            agent = %agent_id,
            removed,
            created,
            "· mirrors reconciled"
        );
    }
}

/// Mirror entries a [`sync`] of `agent_id` would change: missing or stale
/// SkillStar deployments, and SkillStar deployments gone from the source.
/// Foreign entries are never drift.
pub(crate) fn drift(agent_id: &str, source_dir: &Path) -> Vec<PathBuf> {
    let mirrors = crate::agents::global_mirror_dirs(agent_id);
    if mirrors.is_empty() {
        return Vec::new();
    }
    let wanted = managed_deployments(source_dir);
    let reserved = reserved_dirs(source_dir);
    let mut drifted = Vec::new();
    for mirror in mirrors {
        if !mirror.parent().is_some_and(Path::exists)
            || reserved.contains(&fs_ops::canonicalize_existing_prefix(&mirror))
        {
            continue;
        }
        for (name, source) in &wanted {
            let target = mirror.join(name);
            let in_sync = match ownership::owned_deployment(&target, name) {
                Ownership::Link { .. } => {
                    std::fs::canonicalize(&target).ok().as_deref() == Some(source.as_path())
                }
                Ownership::Copy => matches!(
                    (ownership::dir_content_hash(&target), ownership::dir_content_hash(source)),
                    (Ok(a), Ok(b)) if a == b
                ),
                Ownership::Foreign => true,
                Ownership::Missing => false,
            };
            if !in_sync {
                drifted.push(target);
            }
        }
        let Ok(entries) = std::fs::read_dir(&mirror) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !wanted.contains_key(&name)
                && ownership::owned_deployment(&entry.path(), &name).is_owned()
            {
                drifted.push(entry.path());
            }
        }
    }
    drifted
}

/// Directories a mirror must never be: the source itself, the canonical root
/// and every profile's Global skills directory.
fn reserved_dirs(source_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![source_dir.to_path_buf(), paths::agents_skills_root()];
    dirs.extend(
        super::cached_profiles()
            .into_iter()
            .filter(|profile| profile.has_global_skills())
            .map(|profile| profile.global_skills_dir),
    );
    dirs.iter()
        .map(|dir| fs_ops::canonicalize_existing_prefix(dir))
        .collect()
}

/// Managed deployments in `dir` as `name -> resolved source`.
///
/// Links resolve to the hub skill they point at, so mirrors link straight to
/// the hub instead of chaining through another Agent directory; a marked copy
/// mirrors the canonical folder it was copied from. Broken links resolve to
/// nothing and are skipped.
fn managed_deployments(dir: &Path) -> BTreeMap<String, PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let source = match ownership::owned_deployment(&path, &name) {
                Ownership::Link { alive: true } => std::fs::canonicalize(&path).ok()?,
                Ownership::Copy => {
                    std::fs::canonicalize(paths::agents_skills_root().join(&name)).ok()?
                }
                _ => return None,
            };
            Some((name, source))
        })
        .collect()
}

/// What one [`sync_one`] pass changed, aggregated into the single INFO
/// summary emitted by [`sync`].
#[derive(Default)]
struct MirrorChanges {
    created: usize,
    removed: usize,
}

/// Stage a symlink (or copy fallback) beside `target`, then rename it into
/// place. The mirror never deletes the live entry before the replacement exists.
fn place_link_then_rename(source: &Path, target: &Path, name: &str) -> Result<()> {
    let mut staged = materialize::StagedReplace::stage(target, |staging| {
        ownership::deploy_link_or_copy(source, staging, name)?;
        Ok(())
    })?;
    staged.swap()?;
    staged.commit();
    Ok(())
}

fn sync_one(mirror: &Path, wanted: &BTreeMap<String, PathBuf>) -> Result<MirrorChanges> {
    let mut changes = MirrorChanges::default();
    if !wanted.is_empty() {
        std::fs::create_dir_all(mirror)?;
    }

    for (name, source) in wanted {
        let target = mirror.join(name);
        match ownership::owned_deployment(&target, name) {
            Ownership::Link { .. }
                if std::fs::canonicalize(&target).ok().as_deref() == Some(source.as_path()) =>
            {
                continue;
            }
            Ownership::Foreign => {
                warn!(
                    target: "sync",
                    path = %target.display(),
                    "Mirror entry is not SkillStar's (bundled Agent skill?) — leaving it alone"
                );
                continue;
            }
            Ownership::Missing => {
                place_link_then_rename(source, &target, name)?;
                changes.created += 1;
                continue;
            }
            Ownership::Link { .. } | Ownership::Copy => {
                place_link_then_rename(source, &target, name)?;
                changes.removed += 1;
                changes.created += 1;
            }
        }
    }

    // Drop deployments that are gone from the source. Only SkillStar's own
    // entries (links into the managed roots, marked copies): the bundled
    // skills and user links living beside them must survive.
    let Ok(entries) = std::fs::read_dir(mirror) else {
        return Ok(changes);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !wanted.contains_key(&name) && ownership::owned_deployment(&path, &name).is_owned() {
            materialize::remove_entry(&path)?;
            changes.removed += 1;
        }
    }
    Ok(changes)
}

#[cfg(test)]
#[path = "mirror_tests.rs"]
mod tests;
