//! Stat-based fast path for content-baseline re-verification.
//!
//! `ensure_installed_checkout_is_clean` re-reads every byte of every skill
//! sharing a checkout before each fetch, to prove no user edit will be
//! destroyed by `reset --hard`. Byte reads are the dominant cost for large
//! skills on the hot install path. An mtime/size fingerprint of the last
//! trusted snapshot answers the same question with `stat` calls only: any
//! drift (edit, retarget, new or removed file) misses and the caller falls
//! back to the full snapshot — fail-closed semantics unchanged.
//!
//! Trust model, stated plainly: mtime+size equality is the same heuristic
//! `make` and git's index use for "probably unchanged". An edit that
//! deliberately preserves both defeats it; that is outside the accidental-
//! edit threat the cleanliness proof addresses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::content::{SkillSnapshot, SnapshotFileKind};

/// One fingerprinted file: (kind, mtime nanos, size).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct FileStat {
    symlink: bool,
    #[serde(default)]
    mtime_nanos: i128,
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatRecord {
    /// Canonical root the fingerprint was taken under; a retargeted hub link
    /// mismatches and forces the full path.
    root: PathBuf,
    content_hash: String,
    files: BTreeMap<String, FileStat>,
}

fn record_path(name: &str) -> PathBuf {
    skillstar_core::infra::paths::state_dir()
        .join("snapshot-stats")
        .join(format!("{name}.json"))
}

/// Fingerprint a freshly computed snapshot so later probes can trust stats.
///
/// Best effort: a failed write only costs the fast path.
pub(crate) fn record(name: &str, snapshot: &SkillSnapshot) {
    let mut files = BTreeMap::new();
    for file in &snapshot.files {
        let path = snapshot.root.join(&file.relative_path);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return;
        };
        let stat = if file.kind == SnapshotFileKind::Symlink {
            FileStat {
                symlink: true,
                mtime_nanos: 0,
                size: metadata.len(),
            }
        } else {
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos() as i128)
                .unwrap_or(0);
            FileStat {
                symlink: false,
                mtime_nanos: mtime,
                size: metadata.len(),
            }
        };
        files.insert(file.relative_path.clone(), stat);
    }
    let record = StatRecord {
        root: snapshot.root.clone(),
        content_hash: snapshot.content_hash.clone(),
        files,
    };
    let path = record_path(name);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_ok()
        && let Ok(content) = serde_json::to_string(&record)
    {
        let _ = std::fs::write(path, content);
    }
}

/// Does the skill's on-disk state still fingerprint to `expected_hash`?
///
/// `false` for every doubt (missing record, root drift, any stat difference)
/// — the caller then runs the authoritative full snapshot.
pub(crate) fn matches(name: &str, expected_hash: &str) -> bool {
    let Ok(content) = std::fs::read_to_string(record_path(name)) else {
        return false;
    };
    let Ok(record) = serde_json::from_str::<StatRecord>(&content) else {
        return false;
    };
    if record.content_hash != expected_hash {
        return false;
    }
    let skill_entry = skillstar_core::infra::paths::hub_skills_dir().join(name);
    let Ok(canonical) = std::fs::canonicalize(&skill_entry) else {
        return false;
    };
    // The snapshot root sits at (or under) the link target; accept both the
    // recorded root itself and a lockfile-style nested source folder inside
    // it, which is what `resolve_snapshot_root` produced.
    if canonical != record.root && !record.root.starts_with(&canonical) {
        return false;
    }
    let Some(current) = fingerprint(&record.root) else {
        return false;
    };
    current == record.files
}

/// Collect the same file set `content::snapshot` would, but only `stat` it.
fn fingerprint(root: &Path) -> Option<BTreeMap<String, FileStat>> {
    let mut files = BTreeMap::new();
    fingerprint_dir(root, root, 0, &mut files)?;
    Some(files)
}

fn fingerprint_dir(
    root: &Path,
    dir: &Path,
    depth: usize,
    files: &mut BTreeMap<String, FileStat>,
) -> Option<()> {
    if depth > 64 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if crate::content::snapshot_entry_is_ignored(&entry.file_name()) {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return None;
        };
        let relative = path
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        if skillstar_core::infra::fs_ops::is_link(&path) {
            files.insert(
                relative,
                FileStat {
                    symlink: true,
                    mtime_nanos: 0,
                    size: metadata.len(),
                },
            );
        } else if metadata.is_dir() {
            fingerprint_dir(root, &path, depth + 1, files)?;
        } else if metadata.is_file() {
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos() as i128)
                .unwrap_or(0);
            files.insert(
                relative,
                FileStat {
                    symlink: false,
                    mtime_nanos: mtime,
                    size: metadata.len(),
                },
            );
        }
    }
    Some(())
}

/// Probe used by the pre-fetch cleanliness proof: the recorded fingerprint
/// still matches, or the caller must re-read every byte.
pub(crate) fn baseline_unchanged(name: &str, expected_hash: &str) -> bool {
    matches(name, expected_hash)
}

#[cfg(test)]
mod tests;
