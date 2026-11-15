//! Rolling skill archive under `data/skills/backups/` (max 25 snapshots, D-113).
//!
//! Snapshots are taken before destructive or conflict-resolving mutations:
//! uninstalling a skill, manually overwriting a locally modified skill during
//! update, or overwriting a diverged `copy`-deployed skill in an Agent dir.
//! Runtime artifacts (`node_modules`, `.venv`, etc.) and `.git` are excluded so
//! backups stay fast and compact.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use ss_core::infra::{fs_ops, paths};

use crate::library::{self, SkillOrigin};
use crate::skill_lock::{self, SkillLockEntry, SourceType};
use crate::{install_baseline, installed_skill, local_identity, runtime_ignore, skill_mutation};

pub const MAX_BACKUPS: usize = 25;
const BACKUP_METADATA_FILE: &str = "metadata.json";
const BACKUP_CONTENT_DIR: &str = "content";

static BACKUP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupReason {
    Uninstall,
    UpdateOverwrite,
    CopyConflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub id: String,
    pub skill_name: String,
    pub reason: BackupReason,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub called: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SkillOrigin>,
    pub size_bytes: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupMetadata {
    id: String,
    skill_name: String,
    reason: BackupReason,
    created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    called: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<SkillOrigin>,
    size_bytes: u64,
}

fn sanitize_id_segment(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if safe.is_empty() {
        "skill".to_string()
    } else {
        safe
    }
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(meta) = p.symlink_metadata() else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            total = total.saturating_add(dir_size(&p));
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Snapshot `source_dir` into `data/skills/backups/<id>/content` and prune older
/// backups beyond [`MAX_BACKUPS`].
pub fn archive_skill(
    name: &str,
    source_dir: &Path,
    reason: BackupReason,
) -> Result<Option<BackupEntry>> {
    let effective = match std::fs::canonicalize(source_dir) {
        Ok(path) if path.is_dir() => path,
        _ if source_dir.is_dir() => source_dir.to_path_buf(),
        _ => return Ok(None),
    };

    let backups_root = paths::skills_backups_dir();
    std::fs::create_dir_all(&backups_root)
        .with_context(|| format!("Failed to create backups directory {}", backups_root.display()))?;

    let now = chrono::Utc::now();
    let millis = u64::try_from(now.timestamp_millis().max(0)).unwrap_or(0);
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed) % 10_000;
    let id = format!("{millis:016}-{seq:04}-{}", sanitize_id_segment(name));
    let backup_dir = backups_root.join(&id);
    let content_dir = backup_dir.join(BACKUP_CONTENT_DIR);

    if let Err(error) =
        runtime_ignore::copy_dir_excluding_runtime(&effective, &content_dir, &[".skillstar-deploy.json"])
    {
        let _ = fs_ops::remove_dir_all_retry(&backup_dir);
        return Err(error.context(format!("Failed to archive backup for '{name}'")));
    }

    let size_bytes = dir_size(&content_dir);
    let lib = library::load();
    let lock = skill_lock::load();
    let lib_entry = lib.skills.get(name);
    let lock_entry = lock.skills.get(name);

    let metadata = BackupMetadata {
        id: id.clone(),
        skill_name: name.to_string(),
        reason,
        created_at: now.to_rfc3339(),
        source: lib_entry
            .and_then(|e| e.source.clone())
            .or_else(|| lock_entry.map(|e| e.source.clone())),
        source_url: lib_entry
            .and_then(|e| e.source_url.clone())
            .or_else(|| lock_entry.map(|e| e.source_url.clone())),
        called: lib_entry.and_then(|e| e.called.clone()),
        origin: lib_entry
            .map(|e| e.origin)
            .or_else(|| lock_entry.map(|e| SkillOrigin::from(e.source_type)))
            .or_else(|| {
                matches!(local_identity::read_local_identity(&effective), Ok(Some(_)))
                    .then_some(SkillOrigin::Local)
            }),
        size_bytes,
    };

    let meta_json =
        serde_json::to_string_pretty(&metadata).context("Failed to serialize backup metadata")?;
    let meta_path = backup_dir.join(BACKUP_METADATA_FILE);
    if let Err(error) = fs_ops::atomic_write(&meta_path, meta_json.as_bytes()) {
        let _ = fs_ops::remove_dir_all_retry(&backup_dir);
        return Err(error).with_context(|| format!("Failed to write {}", meta_path.display()));
    }

    prune_old_backups(&backups_root, MAX_BACKUPS);

    Ok(Some(BackupEntry {
        id: metadata.id,
        skill_name: metadata.skill_name,
        reason: metadata.reason,
        created_at: metadata.created_at,
        source: metadata.source,
        source_url: metadata.source_url,
        called: metadata.called,
        origin: metadata.origin,
        size_bytes: metadata.size_bytes,
        path: backup_dir,
    }))
}

fn prune_old_backups(backups_root: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(backups_root) else {
        return;
    };
    let mut dirs: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.is_dir() {
                return None;
            }
            let name = e.file_name().to_str()?.to_string();
            Some((name, p))
        })
        .collect();
    // IDs start with `{millis:016}-{seq:04}-...`, so reverse lexical sort puts newest first.
    dirs.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, path) in dirs.into_iter().skip(keep) {
        let _ = fs_ops::remove_dir_all_retry(&path);
    }
}

/// List recorded backups newest-first.
pub fn list_backups() -> Result<Vec<BackupEntry>> {
    let backups_root = paths::skills_backups_dir();
    if !backups_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in std::fs::read_dir(&backups_root)? {
        let entry = entry?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let meta_path = dir.join(BACKUP_METADATA_FILE);
        let Ok(raw) = std::fs::read_to_string(&meta_path) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<BackupMetadata>(&raw) else {
            continue;
        };
        result.push(BackupEntry {
            id: meta.id,
            skill_name: meta.skill_name,
            reason: meta.reason,
            created_at: meta.created_at,
            source: meta.source,
            source_url: meta.source_url,
            called: meta.called,
            origin: meta.origin,
            size_bytes: meta.size_bytes,
            path: dir,
        });
    }
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}

/// Summary `(count, total_bytes)` of backups currently stored on disk.
pub fn backups_summary() -> (usize, u64) {
    let Ok(items) = list_backups() else {
        return (0, 0);
    };
    let bytes = items.iter().map(|i| i.size_bytes).sum();
    (items.len(), bytes)
}

fn allocate_restore_name(desired: &str) -> Result<String> {
    let occupied = |candidate: &str| -> bool {
        paths::agents_skill_dir(candidate).symlink_metadata().is_ok()
            || skill_mutation::policy()
                .managed_repository_for_skill(candidate)
                .ok()
                .flatten()
                .is_some()
    };
    if !occupied(desired) && crate::content::validate_skill_name(desired).is_ok() {
        return Ok(desired.to_string());
    }
    let base = format!("{}-restored", sanitize_id_segment(desired));
    if !occupied(&base) && crate::content::validate_skill_name(&base).is_ok() {
        return Ok(base);
    }
    for n in 2..=999u32 {
        let candidate = format!("{base}-{n}");
        if !occupied(&candidate) && crate::content::validate_skill_name(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    bail!("Could not allocate a free skill name to restore '{desired}'")
}

/// Restore a backup into `data/skills/installed/<name>` (using `<name>-restored`
/// if `<name>` is currently occupied or channel-managed). Returns the restored
/// canonical skill name.
pub fn restore_backup(id: &str) -> Result<String> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        bail!("Invalid backup id '{id}'");
    }
    let backup_dir = paths::skills_backups_dir().join(id);
    let content_dir = backup_dir.join(BACKUP_CONTENT_DIR);
    let meta_path = backup_dir.join(BACKUP_METADATA_FILE);
    if !content_dir.is_dir() {
        bail!("Backup '{id}' content was not found");
    }
    let raw = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("Failed to read backup metadata {}", meta_path.display()))?;
    let meta: BackupMetadata =
        serde_json::from_str(&raw).with_context(|| format!("Invalid backup metadata '{id}'"))?;

    let target_name = allocate_restore_name(&meta.skill_name)?;
    let dest = paths::agents_skill_dir(&target_name);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    runtime_ignore::copy_dir_all(&content_dir, &dest)
        .with_context(|| format!("Failed to restore backup '{id}' into '{}'", dest.display()))?;

    let has_local_sidecar = matches!(local_identity::read_local_identity(&dest), Ok(Some(_)));
    if meta.origin == Some(SkillOrigin::Local) || has_local_sidecar {
        local_identity::ensure_local_identity(&dest)
            .with_context(|| format!("Failed to restore local identity for '{target_name}'"))?;
        library::record_local_skill(&target_name)?;
    } else if let Some(source_url) = meta.source_url.clone().filter(|s| !s.is_empty()) {
        let now = chrono::Utc::now().to_rfc3339();
        let source_type = match meta.origin {
            Some(SkillOrigin::Github) => SourceType::Github,
            Some(SkillOrigin::Git) => SourceType::Git,
            Some(SkillOrigin::Bundle) => SourceType::Bundle,
            Some(SkillOrigin::WellKnown) => SourceType::WellKnown,
            _ => SourceType::Unknown,
        };
        let lock_entry = SkillLockEntry {
            source: meta.source.clone().unwrap_or_default(),
            source_type,
            source_url,
            git_ref: None,
            skill_path: None,
            skill_folder_hash: None,
            installed_at: now.clone(),
            updated_at: now,
            extra: BTreeMap::new(),
        };
        skill_lock::mutate(|lock| {
            lock.upsert(&target_name, lock_entry.clone());
        })?;
        library::record_remote_install(&target_name, &lock_entry, meta.called.clone(), None, false)?;
    } else {
        library::record_local_skill(&target_name)?;
    }

    install_baseline::record(std::slice::from_ref(&target_name));
    installed_skill::invalidate_cache();
    Ok(target_name)
}

/// Remove every backup directory under `data/skills/backups/`, returning
/// `(removed_count, freed_bytes)`.
pub fn clear_all_backups() -> Result<(usize, u64)> {
    let root = paths::skills_backups_dir();
    if !root.is_dir() {
        return Ok((0, 0));
    }
    let mut count = 0usize;
    let mut freed = 0u64;
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        freed = freed.saturating_add(dir_size(&p));
        fs_ops::remove_dir_all_retry(&p)
            .with_context(|| format!("Failed to delete backup directory {}", p.display()))?;
        count += 1;
    }
    Ok((count, freed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_skips_runtime_dirs_prunes_beyond_25_restores_and_clears() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        let skill_dir = paths::agents_skill_dir("demo");
        std::fs::create_dir_all(skill_dir.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(skill_dir.join(".venv")).unwrap();
        std::fs::write(skill_dir.join("node_modules/pkg/a.js"), "big").unwrap();
        std::fs::write(skill_dir.join(".venv/pyvenv.cfg"), "venv").unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: demo\ndescription: demo skill\n---\nhello backup\n",
        )
        .unwrap();

        let first = archive_skill("demo", &skill_dir, BackupReason::Uninstall)
            .unwrap()
            .unwrap();
        assert!(!first.path.join("content/node_modules").exists());
        assert!(!first.path.join("content/.venv").exists());
        assert!(first.path.join("content/SKILL.md").is_file());

        // Trigger pruning (>25 backups).
        for _ in 0..27 {
            archive_skill("demo", &skill_dir, BackupReason::UpdateOverwrite).unwrap();
        }
        let items = list_backups().unwrap();
        assert_eq!(items.len(), MAX_BACKUPS);

        // Restore oldest surviving when `demo` is occupied -> `demo-restored`.
        let restored_name = restore_backup(&items.last().unwrap().id).unwrap();
        assert_eq!(restored_name, "demo-restored");
        assert!(paths::agents_skill_dir("demo-restored").join("SKILL.md").is_file());

        let (cleared, _) = clear_all_backups().unwrap();
        assert_eq!(cleared, MAX_BACKUPS);
        assert!(list_backups().unwrap().is_empty());
    }
}
