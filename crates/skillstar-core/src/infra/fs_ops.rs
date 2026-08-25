//! Cross-platform filesystem operations: symlinks, junction points, directory copies, and retry IO.
//!
//! All modules that need to create/remove symlinks or directory copies
//! **must** use functions from this module.

use anyhow::Context;
use std::path::{Path, PathBuf};

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

/// Remove one regular file with the same Windows transient-lock retry policy.
pub fn remove_file_retry(path: &Path) -> std::io::Result<()> {
    retry_io(|| std::fs::remove_file(path))
}

/// Remove one empty directory with the same Windows transient-lock retry policy.
pub fn remove_dir_retry(path: &Path) -> std::io::Result<()> {
    retry_io(|| std::fs::remove_dir(path))
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
/// pid-suffixed sibling temp file (same directory, so the final `rename`
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
    let tmp = parent.join(format!("{file_name}.skillstar-{}.tmp", std::process::id()));

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
    use super::{atomic_write, create_symlink_or_copy, remove_link_or_copy};
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
