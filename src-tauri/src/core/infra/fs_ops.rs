//! Cross-platform filesystem operations: symlinks, junction points, directory copies, and retry IO.
//!
//! All modules that need to create/remove symlinks or directory copies
//! **must** use functions from this module.

use anyhow::Context;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════
//  Symlink / Junction / Copy operations
// ═══════════════════════════════════════════════════════════════════

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
            // ERROR_PRIVILEGE_NOT_HELD — try junction point fallback
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
///
/// Intended for project-level skill deployment where the project directory
/// may be on a different drive than the hub.  Falls back gracefully:
///
/// 1. Try true symlink (`symlink_dir`)
/// 2. Try junction point (same-drive only)
/// 3. Copy the entire directory tree
///
/// Returns `Ok(true)` if a copy fallback was used (not a live link),
/// `Ok(false)` if a symlink/junction was created.
pub fn create_symlink_or_copy(src: &Path, dst: &Path) -> anyhow::Result<bool> {
    // Safety: never fall back to copying **into** an existing destination.
    // Callers must clear/prepare the target path first.
    if dst.symlink_metadata().is_ok() || is_link(dst) || dst.exists() {
        anyhow::bail!(
            "Destination already exists, refusing to overwrite: {}",
            dst.display()
        );
    }

    match create_symlink(src, dst) {
        Ok(()) => Ok(false),
        Err(_) => {
            // Both symlink and junction failed — copy the directory
            copy_dir_all(src, dst)
                .with_context(|| format!("Failed to copy {:?} -> {:?}", src, dst))?;
            Ok(true)
        }
    }
}

/// Force a directory-tree copy deployment (no symlink / junction).
///
/// Used when the user explicitly chooses **copy** mode for project skills.
/// Refuses to write into an existing destination (same safety contract as
/// [`create_symlink_or_copy`]).
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

/// Recursively copy a directory tree, skipping `.git`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        // Skip .git to keep the copy lightweight
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

/// Check whether a path is a symlink **or** a junction point.
///
/// On Unix this is equivalent to `path.is_symlink()`.
///
/// On Windows, `create_symlink` falls back to junction points when
/// Developer Mode is disabled. Junction points are NTFS reparse points
/// that `Path::is_symlink()` does **not** detect — it only recognises
/// true symbolic links. This helper checks both, ensuring that unlink
/// operations work regardless of which link type was created.
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

/// Resolve a symlink or Windows junction to an absolute target path.
///
/// Relative link targets are joined against `link_path`'s parent. On Windows,
/// falls back to [`junction::get_target`] when [`std::fs::read_link`] fails
/// (typical for directory junctions).
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

/// Cross-platform removal of symlinks **and** junction points.
///
/// On Windows, directory symlinks (created via `symlink_dir`) must be removed
/// with `remove_dir()`, not `remove_file()` — the latter returns
/// `ERROR_ACCESS_DENIED`.  Junction points are removed via `junction::delete`.
///
/// Retries with exponential backoff to handle transient file locks from
/// antivirus scanners, search indexers, etc.
pub fn remove_symlink(path: &Path) -> anyhow::Result<()> {
    tracing::info!(target: "paths", path = %path.display(), "remove_symlink called");

    // Try standard symlink detection first
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
            // On Windows, symlink_metadata().is_dir() can return false for directory
            // symlinks when the target is missing (dangling link). Using the wrong
            // removal function gives ERROR_ACCESS_DENIED. Always try remove_dir first,
            // then fall back to remove_file.
            let remove_op = || -> std::io::Result<()> {
                if is_dir {
                    std::fs::remove_dir(path)
                } else {
                    std::fs::remove_dir(path).or_else(|dir_err| {
                        std::fs::remove_file(path).map_err(|file_err| {
                            // Prefer the dir error if it wasn't just "not a directory"
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

    // On Windows, also check for junction points (created when Developer Mode
    // is disabled and symlink_dir fails with ERROR_PRIVILEGE_NOT_HELD).
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
            retry_io(|| {
                junction::delete(path)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            })
            .with_context(|| format!("Failed to remove junction point: {:?}", path))?;
            return Ok(());
        }
    }

    tracing::error!(target: "paths", path = %path.display(), "Not a symlink or junction");
    anyhow::bail!("Not a symlink or junction: {:?}", path);
}

/// Remove a symlink, junction, or **directory copy** at `path`.
///
/// This is the inverse of `create_symlink_or_copy`: it handles all three
/// link types created on Windows (true symlink, junction, or directory copy).
///
/// For symlinks and junctions, delegates to `remove_symlink`. For real
/// directories (copy fallback), uses `remove_dir_all_retry`.
///
/// The `hub_source_name` is optionally used to verify that the directory
/// looks like a managed skill copy (contains a SKILL.md) before deletion,
/// to avoid accidentally removing unrelated real directories.
pub fn remove_link_or_copy(path: &Path) -> anyhow::Result<()> {
    if is_link(path) {
        return remove_symlink(path);
    }

    // On Windows, junction points that is_link() failed to detect (e.g. due to
    // reparse-point read errors from antivirus or indexer locks) can still be
    // removed via plain remove_dir().  This is safe because:
    //  - For junction points: remove_dir removes only the junction, not the target.
    //  - For real non-empty directories: remove_dir fails with "directory not empty",
    //    so we fall through to the copy-based removal below.
    #[cfg(windows)]
    {
        if path.symlink_metadata().is_ok() {
            if retry_io(|| std::fs::remove_dir(path)).is_ok() {
                return Ok(());
            }
        }
    }

    // Real directory — likely a copy-based deployment.
    // Safety check: only remove if it looks like a skill directory.
    if path.is_dir() {
        // Verify it contains SKILL.md or is an installed skill directory
        // to avoid removing arbitrary user directories.
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

/// Check if the current platform supports symlink creation.
///
/// On Windows, returns true if EITHER symlinks or junction points work.
/// On Unix, always returns true.
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

        // Create test source directory
        let _ = std::fs::create_dir_all(&test_src);

        let result = std::os::windows::fs::symlink_dir(&test_src, &test_dst).is_ok()
            || junction::create(&test_src, &test_dst).is_ok();

        // Clean up
        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        result
    }
}

/// Check if Windows Developer Mode is enabled (true symlinks work).
///
/// Returns `true` on Unix (always supported) or on Windows when
/// `symlink_dir` succeeds without ERROR_PRIVILEGE_NOT_HELD (1314).
///
/// When this returns `false` on Windows, the app falls back to junction
/// points, which have limitations (same-drive only, not detected by
/// `is_symlink()`). The frontend can use this to recommend enabling
/// Developer Mode for a better experience.
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

        // Clean up any leftovers from previous runs
        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        let _ = std::fs::create_dir_all(&test_src);
        let result = std::os::windows::fs::symlink_dir(&test_src, &test_dst).is_ok();

        // Clean up
        let _ = std::fs::remove_dir(&test_dst);
        let _ = std::fs::remove_dir(&test_src);

        result
    }
}

/// Check if two paths are on the same Windows drive letter.
/// Junction points only work within the same drive.
#[cfg(windows)]
fn same_drive(a: &Path, b: &Path) -> bool {
    a.components().next().map_or(false, |ac| {
        b.components().next().map_or(false, |bc| ac == bc)
    })
}

// ═══════════════════════════════════════════════════════════════════
//  Resilient filesystem operations
// ═══════════════════════════════════════════════════════════════════

/// Attempt `remove_dir_all` with retry logic for Windows file-locking.
///
/// On Windows, antivirus software, search indexers, and lingering process
/// handles can hold files open, causing `remove_dir_all` to fail with
/// `ERROR_SHARING_VIOLATION`.  This wrapper retries with exponential
/// backoff (200 → 400 → 800 → 1600 ms, ~3 s total) which handles
/// most transient locks from Windows Defender real-time scanning.
///
/// On Unix this is functionally identical to a single `remove_dir_all` call
/// since file locks do not prevent deletion.
pub fn remove_dir_all_retry(path: &Path) -> std::io::Result<()> {
    retry_io(|| std::fs::remove_dir_all(path))
}

/// Retry an IO operation with exponential backoff.
///
/// 5 attempts total: immediate, then 200ms, 400ms, 800ms, 1600ms delays
/// (~3 seconds maximum wait). Covers transient Windows file locks from
/// antivirus, search indexer, and shell thumbnail generators.
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
        error = %last_err.as_ref().unwrap(),
        "IO operation failed after all retries"
    );
    Err(last_err.unwrap())
}
