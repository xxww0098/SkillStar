//! Cross-platform filesystem operations: symlinks, junction points, directory copies, and retry IO.
//!
//! All modules that need to create/remove symlinks or directory copies
//! **must** use functions from this module.

use anyhow::Context;
use std::path::{Path, PathBuf};

/// Canonicalize the longest existing prefix, then re-append missing tails.
///
/// `std::fs::canonicalize` fails when the last component is gone — the
/// state of a dangling hub link after the checkout already dropped that
/// Skill. On Windows the raw path may also be an 8.3 short name
/// (`RUNNER~1`) while the cache dir canonicalizes to `runneradmin`, so
/// a prefix compare without this helper misses the shared checkout.
pub fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match std::fs::canonicalize(&existing) {
            Ok(mut canonical) => {
                for component in tail.iter().rev() {
                    canonical.push(component);
                }
                return canonical;
            }
            Err(_) => {
                let Some(name) = existing.file_name().map(|name| name.to_os_string()) else {
                    return path.to_path_buf();
                };
                tail.push(name);
                if !existing.pop() {
                    return path.to_path_buf();
                }
            }
        }
    }
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Cross-platform symlink creation (shared utility).
///
/// All modules that need to create symlinks **must** call this function
/// instead of using `std::os::unix::fs::symlink` directly.
///
/// On Windows, `symlink_dir` requires either:
/// - Developer Mode enabled (Settings → Update & Security → For developers)
/// - Or SeCreateSymbolicLinkPrivilege (admin).
///
/// When Developer Mode is unavailable, falls back to junction points
/// (no privilege required, same-drive directories only).
pub fn create_symlink(src: &Path, dst: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(src, dst)
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))?;

    #[cfg(windows)]
    match std::os::windows::fs::symlink_dir(src, dst) {
        Ok(()) => {}
        Err(e) if e.raw_os_error() == Some(1314) => {
            if !same_drive(src, dst) {
                return Err(anyhow::anyhow!(
                    "Symlink creation failed: Developer Mode is required for cross-drive links.\n\
                     Junction points only work within the same drive.\n\
                     Enable Developer Mode in Settings → System → For developers.\n\
                     Source: {:?}, Target: {:?}",
                    src,
                    dst
                ));
            }
            junction::create(src, dst).with_context(|| {
                format!(
                    "Neither symlink nor junction succeeded.\n\
                     Enable Developer Mode for symlink support.\n\
                     Source: {:?}, Target: {:?}",
                    src, dst
                )
            })?;
        }
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst));
        }
    }

    Ok(())
}

/// Recreate a file or directory symlink with its original target text.
///
/// File links must remain file links. On Windows, directory links fall back to
/// a junction when symlink privileges are unavailable so preserving a local
/// copy still works without Developer Mode.
pub fn create_preserved_symlink(
    target: &Path,
    destination: &Path,
    _target_is_dir: bool,
) -> anyhow::Result<()> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, destination).with_context(|| {
        format!(
            "Failed to preserve symlink {:?} -> {:?}",
            destination, target
        )
    })?;

    #[cfg(windows)]
    {
        let result = if _target_is_dir {
            match std::os::windows::fs::symlink_dir(target, destination) {
                Ok(()) => Ok(()),
                Err(error) if error.raw_os_error() == Some(1314) => {
                    let junction_target = if target.is_absolute() {
                        target.to_path_buf()
                    } else {
                        destination
                            .parent()
                            .unwrap_or_else(|| Path::new("."))
                            .join(target)
                    };
                    junction::create(&junction_target, destination)
                }
                Err(error) => Err(error),
            }
        } else {
            std::os::windows::fs::symlink_file(target, destination)
        };
        result.with_context(|| {
            format!(
                "Failed to preserve symlink {:?} -> {:?}",
                destination, target
            )
        })?;
    }

    #[cfg(not(any(unix, windows)))]
    let _ = (target, destination, _target_is_dir);

    Ok(())
}

/// Create a symlink, junction, or **copy** as a last resort.
pub fn create_symlink_or_copy(src: &Path, dst: &Path) -> anyhow::Result<bool> {
    if dst.symlink_metadata().is_ok() || is_link(dst) || dst.exists() {
        anyhow::bail!(
            "Destination already exists, refusing to overwrite: {}",
            dst.display()
        );
    }

    match create_symlink(src, dst) {
        Ok(()) => Ok(false),
        Err(_) => {
            copy_dir_all(src, dst)
                .with_context(|| format!("Failed to copy {:?} -> {:?}", src, dst))?;
            Ok(true)
        }
    }
}

pub fn create_copy_deploy(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if dst.symlink_metadata().is_ok() || is_link(dst) || dst.exists() {
        anyhow::bail!(
            "Destination already exists, refusing to overwrite: {}",
            dst.display()
        );
    }
    copy_dir_all(src, dst).with_context(|| format!("Failed to copy {:?} -> {:?}", src, dst))
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry.file_name() == ".git" {
            continue;
        }

        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

pub fn is_link(path: &Path) -> bool {
    if path.is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        if junction::exists(path).unwrap_or(false) {
            return true;
        }
    }
    false
}

pub fn read_link_resolved(link_path: &Path) -> std::io::Result<PathBuf> {
    let link_target = std::fs::read_link(link_path);
    #[cfg(windows)]
    let link_target = link_target.or_else(|_| junction::get_target(link_path));
    let target = link_target?;
    Ok(if target.is_absolute() {
        target
    } else {
        link_path.parent().unwrap_or(Path::new(".")).join(target)
    })
}

pub fn remove_symlink(path: &Path) -> anyhow::Result<()> {
    tracing::info!(target: "paths", path = %path.display(), "remove_symlink called");

    if path.is_symlink() {
        #[cfg(unix)]
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove symlink: {:?}", path))?;

        #[cfg(windows)]
        {
            let meta = path
                .symlink_metadata()
                .with_context(|| format!("Failed to read symlink metadata: {:?}", path))?;
            let is_dir = meta.is_dir();
            tracing::info!(
                target: "paths",
                path = %path.display(),
                is_dir,
                file_type = ?meta.file_type(),
                "Detected symlink via is_symlink(), attempting removal"
            );
            let remove_op = || -> std::io::Result<()> {
                if is_dir {
                    std::fs::remove_dir(path)
                } else {
                    std::fs::remove_dir(path).or_else(|dir_err| {
                        std::fs::remove_file(path).map_err(|file_err| {
                            tracing::debug!(
                                target: "paths",
                                dir_error = %dir_err,
                                file_error = %file_err,
                                "remove_dir failed, remove_file also failed"
                            );
                            dir_err
                        })
                    })
                }
            };
            retry_io(remove_op).with_context(|| format!("Failed to remove symlink: {:?}", path))?;
        }
        return Ok(());
    }

    #[cfg(windows)]
    {
        let junction_exists = junction::exists(path).unwrap_or(false);
        tracing::info!(
            target: "paths",
            path = %path.display(),
            is_symlink = false,
            junction_exists,
            "path.is_symlink()=false, checking junction"
        );
        if junction_exists {
            tracing::info!(target: "paths", path = %path.display(), "Detected junction point, removing");
            retry_io(|| junction::delete(path).map_err(|e| std::io::Error::other(e)))
                .with_context(|| format!("Failed to remove junction point: {:?}", path))?;
            return Ok(());
        }
    }

    tracing::error!(target: "paths", path = %path.display(), "Not a symlink or junction");
    anyhow::bail!("Not a symlink or junction: {:?}", path);
}

pub fn remove_link_or_copy(path: &Path) -> anyhow::Result<()> {
    if is_link(path) {
        return remove_symlink(path);
    }

    #[cfg(windows)]
    {
        if path.symlink_metadata().is_ok() {
            if retry_io(|| std::fs::remove_dir(path)).is_ok() {
                return Ok(());
            }
        }
    }

    if path.is_dir() {
        let looks_managed = path.join("SKILL.md").exists();
        if looks_managed {
            remove_dir_all_retry(path)?;
            return Ok(());
        }

        anyhow::bail!(
            "Directory exists but does not appear to be a managed skill copy: {:?}",
            path
        );
    }

    anyhow::bail!("Not a symlink, junction, or directory: {:?}", path);
}

pub fn check_developer_mode() -> bool {
    #[cfg(unix)]
    {
        true
    }

    #[cfg(windows)]
    {
        let tmp = std::env::temp_dir();
        let test_src = tmp.join(".skillstar_devmode_test_src");
        let test_dst = tmp.join(".skillstar_devmode_test_dst");

        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        let _ = std::fs::create_dir_all(&test_src);
        let result = std::os::windows::fs::symlink_dir(&test_src, &test_dst).is_ok();

        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        result
    }
}

#[cfg(windows)]
fn same_drive(a: &Path, b: &Path) -> bool {
    a.components()
        .next()
        .is_some_and(|ac| b.components().next().is_some_and(|bc| ac == bc))
}

pub fn remove_dir_all_retry(path: &Path) -> std::io::Result<()> {
    retry_io(|| std::fs::remove_dir_all(path))
}

fn retry_io<F>(op: F) -> std::io::Result<()>
where
    F: Fn() -> std::io::Result<()>,
{
    let delays_ms: &[u64] = &[0, 200, 400, 800, 1600];
    let mut last_err = None;
    for (attempt, &delay) in delays_ms.iter().enumerate() {
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        match op() {
            Ok(()) => {
                if attempt > 0 {
                    tracing::info!(
                        target: "paths",
                        attempt = attempt + 1,
                        "IO operation succeeded after retry"
                    );
                }
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    target: "paths",
                    attempt = attempt + 1,
                    error = %e,
                    os_code = e.raw_os_error().unwrap_or(-1),
                    kind = ?e.kind(),
                    "IO operation failed, will retry"
                );
                last_err = Some(e);
            }
        }
    }
    tracing::error!(
        target: "paths",
        error = %last_err.as_ref().expect("last error exists after retries"),
        "IO operation failed after all retries"
    );
    Err(last_err.expect("last error exists after retries"))
}

/// Atomically replace `path` with `content`.
///
/// The single workspace-wide tmp+rename implementation: writes to a
/// pid+sequence-suffixed sibling temp file (same directory, so the final `rename`
/// cannot cross filesystems), fsyncs it, then renames over the target — a
/// crash mid-write can never leave a truncated target file. Creates the
/// parent directory when missing and cleans the temp file up on failure.
pub fn atomic_write(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    std::fs::create_dir_all(&parent)?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    // Per-process counter: two threads writing the same target must not share
    // a temp path, or one thread's rename ships the other's half-written file.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = parent.join(format!(
        "{file_name}.skillstar-{}-{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));

    let result = (|| {
        #[cfg(unix)]
        let existing_mode = std::fs::metadata(path).ok().map(|m| m.permissions().mode());
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        // Preserve the target's permissions across the rename: File::create
        // yields 0644 (subject to umask), which would silently widen a 0600
        // config — e.g. one holding credentials — to world-readable.
        #[cfg(unix)]
        if let Some(mode) = existing_mode {
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Create a rolling backup of a file before rewriting it (keep last 5).
///
/// Copies the file to `{path}.bak.{timestamp_ms}` next to the original and
/// prunes older backups beyond the 5 most recent. Returns the new backup path.
///
/// Domain-agnostic crash guard: every external tool-config writer (provider
/// store, account switcher, …) funnels through here so a botched merge can
/// always be rolled back to the previous on-disk state.
pub fn create_rolling_backup(path: &Path) -> anyhow::Result<PathBuf> {
    let path_str = path.to_string_lossy().to_string();
    // Millisecond names collide when backups are taken inside one tick — two
    // calls in the same millisecond would silently overwrite each other's
    // backup. Nudge the stamp forward until the name is free; the suffix
    // stays plain digits so `cleanup_old_backups` keeps parsing and ordering
    // it (the nudge is at most a millisecond ahead of the real clock).
    let mut timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut backup_path = PathBuf::from(format!("{}.bak.{}", path_str, timestamp));
    while backup_path.exists() {
        timestamp += 1;
        backup_path = PathBuf::from(format!("{}.bak.{}", path_str, timestamp));
    }

    std::fs::copy(path, &backup_path)
        .with_context(|| format!("Failed to create backup at {}", backup_path.display()))?;

    // Clean up old backups — keep only the 5 most recent
    cleanup_old_backups(path, 5)?;

    Ok(backup_path)
}

/// Remove old `{filename}.bak.{timestamp}` siblings, keeping only the `keep`
/// most recent. Siblings whose suffix is not plain digits are left alone.
pub fn cleanup_old_backups(path: &Path, keep: usize) -> anyhow::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };

    // Pattern: {filename}.bak.{digits}
    let prefix = format!("{}.bak.", file_name);

    let mut backups: Vec<(u128, PathBuf)> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let entry_name = entry.file_name();
            let entry_name_str = entry_name.to_string_lossy();
            if let Some(suffix) = entry_name_str.strip_prefix(&prefix)
                && let Ok(ts) = suffix.parse::<u128>()
            {
                backups.push((ts, entry.path()));
            }
        }
    }

    // Sort by timestamp descending (newest first)
    backups.sort_by_key(|b| std::cmp::Reverse(b.0));

    // Remove backups beyond the keep limit
    for (_ts, backup_path) in backups.iter().skip(keep) {
        let _ = std::fs::remove_file(backup_path);
    }

    Ok(())
}

/// Reveal or open a directory in the operating system's default file manager.
pub fn open_in_file_manager(path: &Path) -> anyhow::Result<()> {
    let path_str = path.to_string_lossy().to_string();

    #[cfg(target_os = "macos")]
    std::process::Command::new("/usr/bin/open")
        .arg(&path_str)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to open folder: {e}"))?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg(&path_str)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to open folder: {e}"))?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open")
        .arg(&path_str)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to open folder: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_write, canonicalize_existing_prefix, create_symlink_or_copy, remove_link_or_copy,
    };
    use tempfile::TempDir;

    #[test]
    fn atomic_write_creates_parent_replaces_target_and_leaves_no_tmp() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("nested").join("config.json");

        atomic_write(&target, b"{\"v\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"v\":1}");

        atomic_write(&target, b"{\"v\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"v\":2}");

        let leftovers: Vec<_> = std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files must not survive: {leftovers:?}"
        );
    }

    #[test]
    fn canonicalize_existing_prefix_keeps_a_missing_tail() {
        let temp = TempDir::new().unwrap();
        let existing = temp.path().join("repos").join("acme");
        std::fs::create_dir_all(&existing).unwrap();
        let missing = existing.join("skills").join("alpha");
        let canonical = canonicalize_existing_prefix(&missing);
        assert_eq!(
            canonical,
            std::fs::canonicalize(&existing)
                .unwrap()
                .join("skills")
                .join("alpha")
        );
    }

    #[test]
    fn copy_fallback_helpers_remove_real_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("SKILL.md"), "# test").unwrap();

        let used_copy = create_symlink_or_copy(&src, &dst).unwrap_or(false);
        if used_copy || !dst.is_symlink() {
            remove_link_or_copy(&dst).unwrap();
            assert!(!dst.exists());
        }
    }

    #[test]
    fn rolling_backup_keeps_only_the_five_most_recent() {
        use super::{cleanup_old_backups, create_rolling_backup};

        let temp = TempDir::new().unwrap();
        let target = temp.path().join("config.json");
        std::fs::write(&target, b"v0").unwrap();

        for round in 1..=7u128 {
            std::fs::write(&target, format!("v{round}")).unwrap();
            let backup = create_rolling_backup(&target).unwrap();
            assert_eq!(backup.parent().unwrap(), target.parent().unwrap());
            let name = backup.file_name().unwrap().to_string_lossy().to_string();
            assert!(name.starts_with("config.json.bak."), "{name}");
        }

        let survivors: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("config.json.bak."))
            .collect();
        assert_eq!(survivors.len(), 5, "keep-last-5 rotation: {survivors:?}");

        // The explicit variant honours its own keep limit.
        cleanup_old_backups(&target, 1).unwrap();
        let survivors = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("config.json.bak.")
            })
            .count();
        assert_eq!(survivors, 1);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_target_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let target = temp.path().join("secret.json");
        std::fs::write(&target, b"old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();

        atomic_write(&target, b"new").unwrap();

        let mode = std::fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "0600 permissions must survive an atomic rewrite"
        );
    }
}
