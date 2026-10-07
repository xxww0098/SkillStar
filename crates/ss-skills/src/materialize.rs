//! The one way Skill folders land on disk.
//!
//! Every writer of a canonical folder, a local adoption or a copy deployment
//! goes through these steps: validate the target name, copy without following
//! links that escape the source boundary, build the result in a hidden
//! same-directory staging folder, then swap it in with a backup that is
//! restored when anything later in the transaction fails. Callers hold
//! [`crate::skill_update::acquire_update_transaction_lock`] across the whole
//! sequence, including the final install-lock write.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use ss_core::infra::fs_ops;

/// Prefix of every transient entry this module creates next to a target.
/// Canonical names never start with `.`, so listings can skip them.
pub(crate) const TRANSIENT_PREFIX: &str = ".skillstar-";

/// The canonical folder name for a Skill identity (frontmatter `name`).
///
/// vercel-labs/skills `sanitizeName` mapping: lowercase, every run outside
/// `[a-z0-9._]` becomes one `-`, leading/trailing `.`/`-` are trimmed, at most
/// 255 bytes. Where vercel would fall back to a shared placeholder this
/// refuses instead: an identity that maps to nothing, or that contains
/// non-ASCII characters (which the mapping would silently drop and make
/// distinct Skills collide), is not installable.
pub fn canonical_skill_name(id: &str) -> Result<String> {
    if !id.is_ascii() {
        bail!("Skill name {id:?} contains non-ASCII characters and has no safe folder name");
    }
    let mut out = String::with_capacity(id.len());
    let mut pending_dash = false;
    for ch in id.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_' {
            if pending_dash {
                out.push('-');
                pending_dash = false;
            }
            out.push(ch);
        } else {
            pending_dash = true;
        }
    }
    let trimmed: String = out
        .trim_matches(|ch| ch == '.' || ch == '-')
        .chars()
        .take(255)
        .collect();
    let trimmed = trimmed.trim_end_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        bail!("Skill name {id:?} has no usable characters for a folder name");
    }
    crate::content::validate_skill_name(&trimmed)
        .map_err(|error| anyhow!("Skill name {id:?} is not a safe folder name: {error}"))?;
    Ok(trimmed)
}

/// Upper bounds for one confined copy. A Skill is a handful of documents and
/// scripts; anything past these is a hostile or broken tree.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CopyLimits {
    pub max_files: u64,
    pub max_bytes: u64,
    pub max_depth: usize,
}

pub(crate) const COPY_LIMITS: CopyLimits = CopyLimits {
    max_files: 10_000,
    max_bytes: 512 * 1024 * 1024,
    max_depth: 32,
};

/// Copy `source` into `dest` (which must not exist yet), excluding `excludes`
/// by entry name at every depth.
///
/// Symlinks (and Windows junctions) are dereferenced only when their resolved
/// target stays inside `boundary` and no component of that target, relative
/// to `boundary`, is an excluded name (`meta -> ../../.git` is skipped like
/// `.git` itself); a link that escapes, or is broken, is skipped. A directory
/// link is followed only to a directory no walk or link has copied yet, so
/// neither link cycles nor many links to one directory can multiply the tree,
/// and the copy fails closed past [`COPY_LIMITS`].
pub(crate) fn copy_confined(
    source: &Path,
    dest: &Path,
    boundary: &Path,
    excludes: &[&str],
) -> Result<()> {
    copy_confined_with_limits(source, dest, boundary, excludes, COPY_LIMITS)
}

pub(crate) fn copy_confined_with_limits(
    source: &Path,
    dest: &Path,
    boundary: &Path,
    excludes: &[&str],
    limits: CopyLimits,
) -> Result<()> {
    let boundary = std::fs::canonicalize(boundary)
        .with_context(|| format!("Failed to resolve source root {}", boundary.display()))?;
    let source = std::fs::canonicalize(source)
        .with_context(|| format!("Failed to resolve source {}", source.display()))?;
    if !source.starts_with(&boundary) {
        bail!(
            "Source folder {} escapes its root {}",
            source.display(),
            boundary.display()
        );
    }
    let mut copy = ConfinedCopy {
        root: &source,
        boundary: &boundary,
        excludes,
        limits,
        visited: HashSet::from([source.clone()]),
        files: 0,
        bytes: 0,
    };
    copy.dir(&source, dest, 0)
}

struct ConfinedCopy<'a> {
    root: &'a Path,
    boundary: &'a Path,
    excludes: &'a [&'a str],
    limits: CopyLimits,
    visited: HashSet<PathBuf>,
    files: u64,
    bytes: u64,
}

impl ConfinedCopy<'_> {
    fn too_large(&self, what: String) -> anyhow::Error {
        anyhow!(
            "Skill folder {} is too large to copy safely ({what}); nothing was installed",
            self.root.display()
        )
    }

    /// Where a link points, if it may be followed.
    fn follow(&self, link: &Path) -> Option<PathBuf> {
        let Ok(resolved) = std::fs::canonicalize(link) else {
            return None;
        };
        let inside = resolved.strip_prefix(self.boundary).ok()?;
        if inside.components().any(|part| {
            self.excludes
                .iter()
                .any(|excluded| part.as_os_str() == *excluded)
        }) {
            tracing::warn!(target: "skills", link = %link.display(), resolved = %resolved.display(), "skipping symlink to excluded content");
            return None;
        }
        Some(resolved)
    }

    fn dir(&mut self, source: &Path, dest: &Path, depth: usize) -> Result<()> {
        if depth > self.limits.max_depth {
            return Err(self.too_large(format!(
                "nested deeper than {} folders",
                self.limits.max_depth
            )));
        }
        std::fs::create_dir_all(dest)?;
        let mut entries = std::fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            if self.excludes.iter().any(|excluded| name == *excluded) {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            let via_link = file_type.is_symlink() || fs_ops::is_link(&path);
            let real = if via_link {
                match self.follow(&path) {
                    Some(real) => real,
                    None => {
                        tracing::warn!(target: "skills", link = %path.display(), "skipping symlink that escapes the source root or is broken");
                        continue;
                    }
                }
            } else {
                path
            };
            let to = dest.join(&name);
            if real.is_dir() {
                // Real subfolders are always copied in place; a link only
                // brings in a folder that no link or walk has copied yet.
                if !self.visited.insert(real.clone()) && via_link {
                    tracing::warn!(target: "skills", path = %real.display(), "skipping a link to a folder that is already copied");
                    continue;
                }
                self.dir(&real, &to, depth + 1)?;
            } else if real.is_file() {
                self.files += 1;
                if self.files > self.limits.max_files {
                    return Err(
                        self.too_large(format!("more than {} files", self.limits.max_files))
                    );
                }
                self.bytes += std::fs::metadata(&real).map(|meta| meta.len()).unwrap_or(0);
                if self.bytes > self.limits.max_bytes {
                    return Err(self.too_large(format!(
                        "more than {} MiB",
                        self.limits.max_bytes / (1024 * 1024)
                    )));
                }
                std::fs::copy(&real, &to)
                    .with_context(|| format!("Failed to copy {}", real.display()))?;
                preserve_executable(&real, &to);
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
fn preserve_executable(from: &Path, to: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = std::fs::metadata(from)
        && let mode = metadata.permissions().mode()
        && mode & 0o111 != 0
    {
        let _ = std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode));
    }
}

#[cfg(not(unix))]
fn preserve_executable(_from: &Path, _to: &Path) {}

/// Remove a path SkillStar already decided it owns: a link is unlinked (never
/// followed), a directory is removed recursively. Missing is success.
pub(crate) fn remove_entry(path: &Path) -> Result<()> {
    let removed = if fs_ops::is_link(path) {
        fs_ops::remove_symlink(path)
    } else {
        match path.symlink_metadata() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                Err(error).with_context(|| format!("Failed to inspect {}", path.display()))
            }
            Ok(metadata) if metadata.is_dir() => fs_ops::remove_dir_all_retry(path)
                .with_context(|| format!("Failed to remove {}", path.display())),
            Ok(_) => std::fs::remove_file(path)
                .with_context(|| format!("Failed to remove {}", path.display())),
        }
    };
    if removed.is_ok() {
        forget_transient_born(path);
    }
    removed
}

fn sibling(dest: &Path, kind: &str) -> Result<PathBuf> {
    let parent = dest
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", dest.display()))?;
    let name = dest
        .file_name()
        .ok_or_else(|| anyhow!("{} has no file name", dest.display()))?
        .to_string_lossy();
    Ok(parent.join(format!(
        "{TRANSIENT_PREFIX}{kind}-{name}-{}",
        uuid::Uuid::new_v4().simple()
    )))
}

/// Copy the directory at `dest` beside it and keep the copy until the caller
/// restores it with [`restore_retained`] or deletes it with [`remove_entry`].
/// Unlike [`StagedReplace`], the copy outlives the call, so a later
/// verification step can still undo a replacement without refetching.
pub(crate) fn retain_copy(dest: &Path) -> Result<PathBuf> {
    let copy = sibling(dest, "retain")?;
    if let Err(error) = copy_confined(dest, &copy, dest, &[]) {
        let _ = remove_entry(&copy);
        forget_transient_born(&copy);
        return Err(error).with_context(|| format!("Failed to back up {}", dest.display()));
    }
    note_transient_born(&copy);
    Ok(copy)
}

/// Put a [`retain_copy`] copy back at `dest`, replacing what is there now.
pub(crate) fn restore_retained(dest: &Path, copy: &Path) -> Result<()> {
    if copy.symlink_metadata().is_err() {
        bail!("The backup of {} is missing", dest.display());
    }
    let aside = if dest.symlink_metadata().is_ok() {
        let aside = sibling(dest, "backup")?;
        rename_retry(dest, &aside)
            .with_context(|| format!("Failed to move {} aside", dest.display()))?;
        note_transient_born(&aside);
        Some(aside)
    } else {
        None
    };
    if let Err(error) = rename_retry(copy, dest) {
        if let Some(aside) = &aside {
            forget_transient_born(aside);
            let _ = rename_retry(aside, dest);
        }
        return Err(error).with_context(|| format!("Failed to restore {}", dest.display()));
    }
    // `copy` was renamed onto `dest`. Its born stamp is a sibling and stays
    // under the old name unless we delete it here.
    forget_transient_born(copy);
    if let Some(aside) = aside {
        if let Err(error) = remove_entry(&aside) {
            tracing::warn!(target: "skills", path = %aside.display(), error = %error, "failed to remove replaced Skill content");
        } else {
            forget_transient_born(&aside);
        }
    }
    Ok(())
}

/// A replacement for `dest` built beside it. Dropping it before [`commit`]
/// undoes everything: the staging folder is removed and, when the swap
/// already happened, the previous entry is put back.
///
/// [`commit`]: StagedReplace::commit
pub(crate) struct StagedReplace {
    dest: PathBuf,
    staging: Option<PathBuf>,
    backup: Option<PathBuf>,
    swapped: bool,
    done: bool,
}

impl StagedReplace {
    pub(crate) fn stage(dest: &Path, fill: impl FnOnce(&Path) -> Result<()>) -> Result<Self> {
        let staging = sibling(dest, "stage")?;
        let mut staged = Self {
            dest: dest.to_path_buf(),
            staging: Some(staging.clone()),
            backup: None,
            swapped: false,
            done: false,
        };
        fill(&staging).with_context(|| format!("Failed to stage {}", dest.display()))?;
        if staging.symlink_metadata().is_err() {
            staged.staging = None;
            bail!("Staging for {} produced nothing", dest.display());
        }
        note_transient_born(&staging);
        Ok(staged)
    }

    /// Move the previous entry aside and the staged one into place.
    pub(crate) fn swap(&mut self) -> Result<()> {
        let staging = self
            .staging
            .clone()
            .ok_or_else(|| anyhow!("{} has nothing staged", self.dest.display()))?;
        if self.dest.symlink_metadata().is_ok() {
            let backup = sibling(&self.dest, "backup")?;
            rename_retry(&self.dest, &backup)
                .with_context(|| format!("Failed to move {} aside", self.dest.display()))?;
            // Rename keeps the directory's old mtime. The born stamp is the
            // crash clock sweep reads, not that mtime.
            note_transient_born(&backup);
            self.backup = Some(backup);
        }
        if let Err(error) = rename_retry(&staging, &self.dest) {
            if let Some(backup) = self.backup.take() {
                forget_transient_born(&backup);
                let _ = rename_retry(&backup, &self.dest);
            }
            return Err(error).with_context(|| {
                format!("Failed to move staged {} into place", self.dest.display())
            });
        }
        // The stage directory was renamed into place. Its born stamp is a
        // sibling and does not move with it.
        forget_transient_born(&staging);
        self.staging = None;
        self.swapped = true;
        Ok(())
    }

    /// Keep the new entry and discard the backup of the old one.
    pub(crate) fn commit(mut self) {
        self.done = true;
        if let Some(backup) = self.backup.take() {
            if let Err(error) = remove_entry(&backup) {
                tracing::warn!(
                    target: "skills",
                    path = %backup.display(),
                    error = %error,
                    "failed to remove replaced Skill backup"
                );
            } else {
                forget_transient_born(&backup);
            }
        }
    }

    /// The previous entry, after [`swap`] and before [`commit`].
    ///
    /// [`swap`]: StagedReplace::swap
    /// [`commit`]: StagedReplace::commit
    pub(crate) fn backup_path(&self) -> Option<&Path> {
        self.backup.as_deref()
    }

    fn rollback(&mut self) {
        if let Some(staging) = self.staging.take() {
            let _ = remove_entry(&staging);
            forget_transient_born(&staging);
        }
        if self.swapped {
            let _ = remove_entry(&self.dest);
            self.swapped = false;
        }
        if let Some(backup) = self.backup.take() {
            forget_transient_born(&backup);
            let _ = rename_retry(&backup, &self.dest);
        }
    }
}

impl Drop for StagedReplace {
    fn drop(&mut self) {
        if !self.done {
            self.rollback();
        }
    }
}

/// How long a hidden `.skillstar-stage-*` / `-backup-*` / `-remove-*` /
/// `-retain-*` entry may sit beside a Skill before the transaction holder
/// deletes it. The clock starts when the residual is created (the born
/// stamp), not at the directory mtime rename preserves. Live writers hold
/// the transaction for the whole swap, so anything older than this belongs
/// to a process that died.
pub const STALE_TRANSIENT_AGE: Duration = Duration::from_secs(30 * 60);

/// Sibling of a transient whose contents are the unix seconds it was created.
/// Sweep reads this file. A same-directory rename keeps the old directory
/// mtime, so that mtime is not a crash clock.
const BORN_PREFIX: &str = ".skillstar-born-";

const TRANSIENT_KINDS: &[&str] = &["stage", "backup", "remove", "retain"];

/// What [`sweep_stale_transients`] removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransientSweep {
    pub removed: usize,
}

/// Delete crashed staging leftovers in the canonical root and every Agent
/// Global skills directory. Callers hold the skill transaction.
pub(crate) fn sweep_stale_transients(max_age: Duration) -> TransientSweep {
    sweep_stale_transients_at(max_age, SystemTime::now())
}

pub(crate) fn sweep_stale_transients_at(max_age: Duration, now: SystemTime) -> TransientSweep {
    // `cargo test` compiles this into the library. A test that takes the
    // transaction without a sandbox must not walk the developer's Agent
    // directories and delete old staging leftovers there.
    #[cfg(test)]
    if std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME").is_none() {
        return TransientSweep { removed: 0 };
    }
    let mut seen = HashSet::new();
    let mut dirs = Vec::new();
    {
        let mut push = |dir: PathBuf| {
            let key = fs_ops::canonicalize_existing_prefix(&dir);
            if seen.insert(key) {
                dirs.push(dir);
            }
        };
        push(ss_core::infra::paths::agents_skills_root());
        for profile in crate::agents::list_profiles() {
            if profile.has_global_skills() {
                push(profile.global_skills_dir);
            }
        }
    }
    let removed = dirs
        .iter()
        .map(|dir| sweep_transient_dir(dir, max_age, now))
        .sum();
    TransientSweep { removed }
}

/// Run [`sweep_stale_transients`] at most once every ten minutes. Transaction
/// entry calls this on the outermost acquire.
pub(crate) fn sweep_stale_transients_now_and_then() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST_UNIX_SECS: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let previous = LAST_UNIX_SECS.load(Ordering::Relaxed);
    if previous != 0 && now.saturating_sub(previous) < 10 * 60 {
        return;
    }
    LAST_UNIX_SECS.store(now, Ordering::Relaxed);
    let sweep = sweep_stale_transients(STALE_TRANSIENT_AGE);
    if sweep.removed > 0 {
        tracing::info!(
            target: "skills",
            removed = sweep.removed,
            "removed stale skill staging directories"
        );
    }
}

fn sweep_transient_dir(dir: &Path, max_age: Duration, now: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let entries: Vec<_> = entries.flatten().collect();
    let mut removed = 0;
    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !is_transient_residue(&name) {
            continue;
        }
        let path = entry.path();
        if !transient_is_stale(&path, max_age, now) || !sweep_may_delete(&name, &path) {
            continue;
        }
        if remove_entry(&path).is_ok() {
            forget_transient_born(&path);
            removed += 1;
        }
    }
    for entry in &entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Some(transient_name) = name.strip_prefix(BORN_PREFIX) else {
            continue;
        };
        if dir.join(transient_name).symlink_metadata().is_err() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    removed
}

/// Age is the born stamp. A missing stamp is not stale: directory mtime is
/// the pre-rename content time and would delete a backup that was just created.
fn transient_is_stale(path: &Path, max_age: Duration, now: SystemTime) -> bool {
    transient_born(path)
        .and_then(|born| born.checked_add(max_age))
        .is_some_and(|deadline| deadline < now)
}

/// `backup` / `remove` / `retain` whose destination is gone are the only
/// remaining copy. Doctor restores them. A stage is deleted only when its
/// destination is still there. A backup or retain is deleted only when that
/// destination is still there and the replacement is known to have committed.
fn sweep_may_delete(name: &str, path: &Path) -> bool {
    let Some((kind, skill)) = transient_kind_and_skill(name) else {
        return false;
    };
    let Some(skill) = skill else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let dest = parent.join(skill);
    let target_exists = dest.symlink_metadata().is_ok();
    match kind {
        "stage" => target_exists,
        "backup" | "retain" => target_exists && transient_replacement_committed(kind, &dest, path),
        _ => false,
    }
}

/// Split `.skillstar-{kind}-{skill}[-{uuid}]` into its kind and skill folder.
pub(crate) fn transient_kind_and_skill(name: &str) -> Option<(&str, Option<&str>)> {
    let rest = name.strip_prefix(TRANSIENT_PREFIX)?;
    let (kind, rest) = rest.split_once('-').unwrap_or((rest, ""));
    let skill = match rest.rsplit_once('-') {
        Some((skill, uuid)) if uuid.len() == 32 && uuid.chars().all(|c| c.is_ascii_hexdigit()) => {
            skill
        }
        _ => rest,
    };
    Some((kind, Some(skill).filter(|skill| !skill.is_empty())))
}

/// Whether the bytes now at `dest` are the committed replacement of `residual`.
///
/// A backup is committed when the install baseline matches `dest` (that record
/// is written only after the swap's commit). A channel retain is committed
/// only when the subscription baseline matches `dest` and the install lock's
/// ref matches that subscription. A subscription or lock that still describes
/// the retained bytes, or any failure to prove the new content, keeps it.
pub(crate) fn transient_replacement_committed(kind: &str, dest: &Path, residual: &Path) -> bool {
    let Some(skill) = dest.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if kind == "retain" {
        return retain_replacement_committed(skill, dest, residual);
    }
    let Ok(dest_hash) =
        crate::content::snapshot_path(skill, dest).map(|snapshot| snapshot.content_hash)
    else {
        return false;
    };
    crate::install_baseline::recorded_hash(skill).as_deref() == Some(dest_hash.as_str())
}

fn retain_replacement_committed(skill: &str, dest: &Path, residual: &Path) -> bool {
    use crate::channels::shared_channels::ChannelSubscriptionRegistry;

    let Ok(dest_hash) =
        crate::content::snapshot_path(skill, dest).map(|snapshot| snapshot.content_hash)
    else {
        return false;
    };
    let Ok(retained_hash) =
        crate::content::snapshot_path(skill, residual).map(|snapshot| snapshot.content_hash)
    else {
        return false;
    };
    let Ok(store) =
        crate::channels::shared_channels::DiskChannelSubscriptionRegistry.load_mutable()
    else {
        return false;
    };
    let Some(subscribed) = store
        .subscriptions
        .iter()
        .flat_map(|subscription| subscription.skills.iter())
        .find(|skill_row| skill_row.id == skill)
    else {
        return false;
    };
    if subscribed.baseline_hash == retained_hash || subscribed.release_content_hash == retained_hash
    {
        return false;
    }
    let lock = crate::skill_lock::load();
    let Some((_, entry)) = lock.entry_for_folder(skill) else {
        return false;
    };
    entry.git_ref.as_deref() == Some(subscribed.provenance.git_ref.as_str())
        && subscribed.baseline_hash == dest_hash
        && subscribed.baseline_hash != retained_hash
}

pub(crate) fn note_transient_born(path: &Path) {
    write_transient_born_at(path, SystemTime::now());
}

pub(crate) fn write_transient_born_at(path: &Path, when: SystemTime) {
    let Some(stamp) = born_stamp_path(path) else {
        return;
    };
    let secs = when
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(stamp, secs.to_string());
}

fn transient_born(path: &Path) -> Option<SystemTime> {
    let stamp = born_stamp_path(path)?;
    let secs: u64 = std::fs::read_to_string(stamp).ok()?.trim().parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn born_stamp_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.parent()?.join(format!("{BORN_PREFIX}{name}")))
}

pub(crate) fn forget_transient_born(path: &Path) {
    if let Some(stamp) = born_stamp_path(path) {
        let _ = std::fs::remove_file(stamp);
    }
}

fn is_transient_residue(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(TRANSIENT_PREFIX) else {
        return false;
    };
    TRANSIENT_KINDS
        .iter()
        .any(|kind| rest.starts_with(&format!("{kind}-")))
}

const RENAME_RETRY_DELAYS_MS: &[u64] = &[0, 30, 60, 120];

fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    retry_attempts(RENAME_RETRY_DELAYS_MS, || std::fs::rename(from, to))
}

fn retry_attempts(
    delays_ms: &[u64],
    mut op: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut last = None;
    for (attempt, delay) in delays_ms.iter().enumerate() {
        if *delay > 0 {
            std::thread::sleep(Duration::from_millis(*delay));
        }
        match op() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(error),
            Err(error) => {
                tracing::warn!(
                    target: "skills",
                    attempt = attempt + 1,
                    error = %error,
                    "rename failed, will retry"
                );
                last = Some(error);
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename was not attempted")))
}

#[cfg(test)]
#[path = "materialize_tests.rs"]
mod tests;
