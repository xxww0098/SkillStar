//! Cross-platform filesystem operations: symlinks, junction points, directory copies, and retry IO.
//!
//! All modules that need to create/remove symlinks or directory copies
//! **must** use functions from this module.

use anyhow::Context;
use std::path::{Path, PathBuf};

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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn check_symlink_support() -> bool {
    #[cfg(unix)]
    {
        true
    }

    #[cfg(windows)]
    {
        let tmp = std::env::temp_dir();
        let test_src = tmp.join(".skillstar_symlink_test_src");
        let test_dst = tmp.join(".skillstar_symlink_test_dst");

        let _ = std::fs::create_dir_all(&test_src);

        let result = std::os::windows::fs::symlink_dir(&test_src, &test_dst).is_ok()
            || junction::create(&test_src, &test_dst).is_ok();

        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        result
    }
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

#[cfg(test)]
mod tests {
    use super::{create_symlink_or_copy, remove_link_or_copy};
    use tempfile::TempDir;

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
}
