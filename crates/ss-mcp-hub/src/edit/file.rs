use crate::error::{McpHubError, McpHubResult};
use chrono::Local;
use serde::{Deserialize, Serialize};
use ss_core::infra::paths::mcp_backups_dir;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const MAX_BACKUPS: usize = 40;
const ABSENT_MARKER_BYTES: &[u8] = b"# skillstar-empty-file-marker: target did not exist before write\n";
static BACKUP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub id: String,
    pub file_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    pub created_at: String,
    pub size_bytes: u64,
    pub existed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRestoreResult {
    pub id: String,
    pub target_path: String,
    pub removed_new_file: bool,
    pub restored_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupMeta {
    id: String,
    target_path: String,
    created_at: String,
    existed: bool,
}

/// Recorded file-level formatting metadata preserved across edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextPreservation {
    pub has_bom: bool,
    pub crlf: bool,
}

impl TextPreservation {
    pub fn from_bytes(raw: &[u8]) -> Self {
        let stripped = raw.strip_prefix(UTF8_BOM).unwrap_or(raw);
        let text = String::from_utf8_lossy(stripped);
        let crlf = text.contains("\r\n");
        Self {
            has_bom: raw.starts_with(UTF8_BOM),
            crlf,
        }
    }

    pub fn decode_lf(&self, raw: &[u8]) -> String {
        let stripped = raw.strip_prefix(UTF8_BOM).unwrap_or(raw);
        let text = String::from_utf8_lossy(stripped);
        if self.crlf {
            text.replace("\r\n", "\n")
        } else {
            text.into_owned()
        }
    }

    pub fn encode(&self, lf_body: &str) -> Vec<u8> {
        let normalized_body = if self.crlf {
            lf_body.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            lf_body.replace("\r\n", "\n")
        };
        let mut out = Vec::with_capacity(
            normalized_body.len() + if self.has_bom { UTF8_BOM.len() } else { 0 },
        );
        if self.has_bom {
            out.extend_from_slice(UTF8_BOM);
        }
        out.extend_from_slice(normalized_body.as_bytes());
        out
    }
}

/// Follow symlinks (including dangling tail links) up to 255 hops so writes
/// go to the target file instead of replacing the symlink with a regular file.
pub fn resolve_symlink_target(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    let mut current = path.to_path_buf();
    for _ in 0..255 {
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => match fs::read_link(&current) {
                Ok(dest) => {
                    current = if dest.is_relative() {
                        current
                            .parent()
                            .map(|p| p.join(&dest))
                            .unwrap_or(dest)
                    } else {
                        dest
                    };
                }
                Err(_) => break,
            },
            _ => break,
        }
    }
    current
}

#[cfg(unix)]
fn is_multi_hardlink(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .map(|m| m.nlink() > 1)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_multi_hardlink(_path: &Path) -> bool {
    false
}

/// Read an existing config file losslessly without creating any parent directories.
/// Returns `Ok(None)` when the target does not exist yet.
pub fn read_lossless(path: &Path) -> McpHubResult<Option<(Vec<u8>, String, TextPreservation, PathBuf)>> {
    let target = resolve_symlink_target(path);
    match fs::read(&target) {
        Ok(raw) => {
            let pres = TextPreservation::from_bytes(&raw);
            let text = pres.decode_lf(&raw);
            Ok(Some((raw, text, pres, target)))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(McpHubError::Io(err)),
    }
}

/// Persist a rolling backup copy of `raw` into `~/.skillstar/data/mcp/backups/`.
pub fn create_rolling_backup(target: &Path, raw: &[u8]) -> McpHubResult<()> {
    create_rolling_backup_inner(target, raw, true)
}

fn create_absent_marker_backup(target: &Path) -> McpHubResult<()> {
    create_rolling_backup_inner(target, ABSENT_MARKER_BYTES, false)
}

fn create_rolling_backup_inner(target: &Path, raw: &[u8], existed: bool) -> McpHubResult<()> {
    let backup_root = mcp_backups_dir();
    fs::create_dir_all(&backup_root)?;
    let now = Local::now();
    let ts = now.format("%Y%m%d-%H%M%S-%3f").to_string();
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed) % 10_000;
    let base = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config");
    let sanitized: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let suffix = if existed { "bak" } else { "absent.bak" };
    let file_name = format!("{ts}-{seq:04}-{sanitized}.{suffix}");
    let backup_path = backup_root.join(&file_name);
    fs::write(&backup_path, raw)?;

    let meta_dir = backup_root.join(".meta");
    if fs::create_dir_all(&meta_dir).is_ok() {
        let meta = BackupMeta {
            id: file_name.clone(),
            target_path: target.to_string_lossy().to_string(),
            created_at: now.to_rfc3339(),
            existed,
        };
        if let Ok(encoded) = serde_json::to_vec(&meta) {
            let _ = fs::write(meta_dir.join(format!("{file_name}.json")), encoded);
        }
    }

    prune_old_backups(&backup_root, MAX_BACKUPS);
    Ok(())
}

fn prune_old_backups(backup_root: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(backup_root) else {
        return;
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                Some((name, path))
            } else {
                None
            }
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let drop_count = files.len().saturating_sub(keep);
    let meta_dir = backup_root.join(".meta");
    for (name, path) in files.into_iter().take(drop_count) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(meta_dir.join(format!("{name}.json")));
    }
}

/// List rolling backups stored in `data/mcp/backups/`, newest first.
pub fn list_backups(limit: Option<usize>) -> McpHubResult<Vec<BackupEntry>> {
    let backup_root = mcp_backups_dir();
    let entries = match fs::read_dir(&backup_root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(McpHubError::Io(err)),
    };
    let meta_dir = backup_root.join(".meta");
    let mut items = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.starts_with('.') {
            continue;
        }
        let Ok(meta_fs) = entry.metadata() else {
            continue;
        };
        let sidecar = fs::read(meta_dir.join(format!("{file_name}.json")))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<BackupMeta>(&bytes).ok());
        let existed = sidecar
            .as_ref()
            .map(|m| m.existed)
            .unwrap_or_else(|| !file_name.ends_with(".absent.bak"));
        let created_at = sidecar
            .as_ref()
            .map(|m| m.created_at.clone())
            .unwrap_or_else(|| file_name.chars().take(19).collect());
        items.push(BackupEntry {
            id: file_name.clone(),
            file_name,
            target_path: sidecar.map(|m| m.target_path),
            created_at,
            size_bytes: if existed { meta_fs.len() } else { 0 },
            existed,
        });
    }
    items.sort_by(|a, b| b.id.cmp(&a.id));
    if let Some(max) = limit {
        items.truncate(max);
    }
    Ok(items)
}

/// Restore a rolling backup by id (and optional explicit destination path override).
pub fn restore_backup(
    backup_id: &str,
    target_override: Option<&Path>,
) -> McpHubResult<BackupRestoreResult> {
    let trimmed = backup_id.trim();
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains("..")
    {
        return Err(McpHubError::InvalidServer(format!(
            "invalid backup id '{backup_id}'"
        )));
    }
    let backup_root = mcp_backups_dir();
    let backup_path = backup_root.join(trimmed);
    if !backup_path.is_file() {
        return Err(McpHubError::NotFound(format!("backup '{trimmed}'")));
    }

    let sidecar = fs::read(backup_root.join(".meta").join(format!("{trimmed}.json")))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BackupMeta>(&bytes).ok());

    let dest = match target_override {
        Some(p) => p.to_path_buf(),
        None => sidecar
            .as_ref()
            .map(|m| PathBuf::from(&m.target_path))
            .ok_or_else(|| {
                McpHubError::InvalidServer(format!(
                    "backup '{trimmed}' has no recorded target path; pass an explicit target path"
                ))
            })?,
    };

    let existed = sidecar
        .as_ref()
        .map(|m| m.existed)
        .unwrap_or_else(|| !trimmed.ends_with(".absent.bak"));
    let target = resolve_symlink_target(&dest);

    if !existed {
        if target.exists() {
            let current = fs::read(&target)?;
            create_rolling_backup(&target, &current)?;
            fs::remove_file(&target)?;
        }
        return Ok(BackupRestoreResult {
            id: trimmed.to_string(),
            target_path: dest.to_string_lossy().to_string(),
            removed_new_file: true,
            restored_bytes: 0,
        });
    }

    let raw = fs::read(&backup_path)?;
    if target.exists() {
        let current = fs::read(&target)?;
        if current != raw {
            create_rolling_backup(&target, &current)?;
        }
    }
    if target.exists() && is_multi_hardlink(&target) {
        write_in_place(&target, &raw)?;
    } else {
        write_atomic(&target, &raw)?;
    }
    Ok(BackupRestoreResult {
        id: trimmed.to_string(),
        target_path: dest.to_string_lossy().to_string(),
        removed_new_file: false,
        restored_bytes: raw.len() as u64,
    })
}

/// Lossless write:
/// - No-op if final encoded bytes match `original_raw` byte-for-byte.
/// - Follows symlinks cleanly so the symlink itself stays intact.
/// - Writes in-place if the file has multiple hardlinks.
/// - Creates rolling backup before overwriting an existing file (or empty marker before creating a new file).
///
/// Returns `Ok(true)` if bytes actually changed on disk.
pub fn write_lossless_with_backup(
    path: &Path,
    original_raw: Option<&[u8]>,
    new_lf_body: &str,
    pres: &TextPreservation,
) -> McpHubResult<bool> {
    let target = resolve_symlink_target(path);
    let encoded = pres.encode(new_lf_body);

    if let Some(prev) = original_raw
        && prev == encoded.as_slice()
    {
        return Ok(false);
    }

    if target.exists() {
        let current_disk = fs::read(&target)?;
        if current_disk == encoded {
            return Ok(false);
        }
        create_rolling_backup(&target, &current_disk)?;
    } else {
        create_absent_marker_backup(&target)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
    }

    if target.exists() && is_multi_hardlink(&target) {
        write_in_place(&target, &encoded)?;
    } else {
        write_atomic(&target, &encoded)?;
    }
    Ok(true)
}

fn write_atomic(target: &Path, bytes: &[u8]) -> McpHubResult<()> {
    let parent = target.parent().ok_or_else(|| {
        McpHubError::ConfigEdit {
            path: target.display().to_string(),
            detail: "missing parent directory".to_string(),
        }
    })?;
    fs::create_dir_all(parent)?;
    let existing_perms = fs::metadata(target).ok().map(|m| m.permissions());

    let pid = std::process::id();
    let nonce = Local::now().timestamp_nanos_opt().unwrap_or(0);
    let tmp_name = format!(
        ".{}.{}.{}.tmp",
        target
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("mcp"),
        pid,
        nonce
    );
    let tmp_path = parent.join(tmp_name);
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Some(perms) = existing_perms {
        let _ = fs::set_permissions(&tmp_path, perms);
    }
    fs::rename(&tmp_path, target)?;
    Ok(())
}

fn write_in_place(target: &Path, bytes: &[u8]) -> McpHubResult<()> {
    let mut f = fs::OpenOptions::new().write(true).open(target)?;
    f.write_all(bytes)?;
    f.set_len(bytes.len() as u64)?;
    f.sync_all()?;
    Ok(())
}
