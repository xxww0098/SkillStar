//! Centralised path resolution for all SkillStar storage locations.
//!
//! Every module that needs a filesystem path **must** go through this module
//! instead of calling `dirs::data_dir()` / `dirs::home_dir()` directly.
//!
//! ## Directory structure (v2)
//!
//! ```text
//! ~/.skillstar/               # data_root()
//! ├── config/                 # User-editable configuration files
//! ├── db/                     # SQLite databases
//! ├── logs/                   # Runtime and per-run logs
//! ├── state/                  # Rebuildable runtime state & metadata
//! └── hub/                    # hub_root() — skill hub, repos, lockfile
//!     ├── skills/             # Central symlink index
//!     ├── local/              # User-authored local skills
//!     ├── repos/              # Cached git repositories
//!     ├── publish/            # Publish staging area
//!     └── lock.json           # Installation lockfile
//! ```
//!
//! ## Environment variable overrides
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `SKILLSTAR_DATA_DIR` | `~/.skillstar` | App config & metadata root |
//! | `SKILLSTAR_HUB_DIR` | `~/.skillstar/hub` | Skill hub, repo cache, lockfile |
//!
//! Setting these variables during development keeps dev data completely
//! separate from the production (installed) app.

use anyhow::Context;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Whether legacy migration has already run this session.
static MIGRATION_DONE: OnceLock<()> = OnceLock::new();

// ═══════════════════════════════════════════════════════════════════
//  Root directories
// ═══════════════════════════════════════════════════════════════════

/// App root — all SkillStar data lives under here.
///
/// Default: `~/.skillstar/` (all platforms)
/// Override: `SKILLSTAR_DATA_DIR`
pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_DATA_DIR") {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
}

/// Hub root — skills, repo cache, lockfile, publish cache live here.
///
/// Default: `~/.skillstar/hub`
/// Override: `SKILLSTAR_HUB_DIR`
pub fn hub_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR") {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    data_root().join("hub")
}

/// User home directory (used for agent profile dirs like `~/.claude/skills`).
pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

// ═══════════════════════════════════════════════════════════════════
//  Categorised sub-directories under data_root()
// ═══════════════════════════════════════════════════════════════════

/// `~/.skillstar/config/` — user-editable configuration files.
pub fn config_dir() -> PathBuf {
    data_root().join("config")
}

/// `~/.skillstar/db/` — all SQLite databases.
pub fn db_dir() -> PathBuf {
    data_root().join("db")
}

/// `~/.skillstar/logs/` — runtime and per-run logs.
pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

/// `~/.skillstar/state/` — rebuildable runtime state & metadata.
pub fn state_dir() -> PathBuf {
    data_root().join("state")
}

// ═══════════════════════════════════════════════════════════════════
//  Config file paths  (config/)
// ═══════════════════════════════════════════════════════════════════

/// `config/ai.json` — AI provider configuration.
pub fn ai_config_path() -> PathBuf {
    config_dir().join("ai.json")
}

/// `config/acp.json` — ACP agent configuration.
pub fn acp_config_path() -> PathBuf {
    config_dir().join("acp.json")
}

/// `config/proxy.json` — proxy configuration.
pub fn proxy_config_path() -> PathBuf {
    config_dir().join("proxy.json")
}

/// `config/github_mirror.json` — GitHub mirror/accelerator configuration.
pub fn github_mirror_config_path() -> PathBuf {
    config_dir().join("github_mirror.json")
}

/// `config/profiles.toml` — agent profile definitions.
pub fn profiles_config_path() -> PathBuf {
    config_dir().join("profiles.toml")
}

/// `config/scan_policy.yaml` — security scan policy.
pub fn security_scan_policy_path() -> PathBuf {
    config_dir().join("scan_policy.yaml")
}

// ═══════════════════════════════════════════════════════════════════
//  Database paths  (db/)
// ═══════════════════════════════════════════════════════════════════

/// `db/marketplace.db` — local-first marketplace snapshot DB.
pub fn marketplace_db_path() -> PathBuf {
    db_dir().join("marketplace.db")
}

/// `db/translation.db` — translation cache DB.
pub fn translation_db_path() -> PathBuf {
    db_dir().join("translation.db")
}

/// `db/security.db` — security scan cache DB.
pub fn security_scan_db_path() -> PathBuf {
    db_dir().join("security.db")
}

// ═══════════════════════════════════════════════════════════════════
//  Log paths  (logs/)
// ═══════════════════════════════════════════════════════════════════

/// `logs/security.log` — rolling security scan runtime log.
pub fn security_scan_log_path() -> PathBuf {
    logs_dir().join("security.log")
}

/// `logs/scans/` — per-run timestamped security scan reports.
pub fn security_scan_logs_dir() -> PathBuf {
    logs_dir().join("scans")
}

// ═══════════════════════════════════════════════════════════════════
//  State paths  (state/)
// ═══════════════════════════════════════════════════════════════════

/// `state/patrol.json` — patrol background-run state.
pub fn patrol_state_path() -> PathBuf {
    state_dir().join("patrol.json")
}

/// `state/projects.json` — registered projects manifest.
pub fn projects_manifest_path() -> PathBuf {
    state_dir().join("projects.json")
}

/// `state/projects/<name>` — per-project detail directory.
pub fn project_detail_dir(name: &str) -> PathBuf {
    state_dir().join("projects").join(name)
}

/// `state/groups.json` — skill groups.
pub fn groups_path() -> PathBuf {
    state_dir().join("groups.json")
}

/// `state/packs.json` — skill packs registry.
pub fn packs_path() -> PathBuf {
    state_dir().join("packs.json")
}

/// `state/repo_history.json` — repo import history.
pub fn repo_history_path() -> PathBuf {
    state_dir().join("repo_history.json")
}

/// `state/mymemory_usage.json` — MyMemory API usage tracking.
pub fn mymemory_usage_path() -> PathBuf {
    state_dir().join("mymemory_usage.json")
}

/// `state/mymemory_disabled` — MyMemory disable flag.
pub fn mymemory_disabled_path() -> PathBuf {
    state_dir().join("mymemory_disabled")
}

/// `state/batch_translate_pending.json` — pending batch translations.
pub fn batch_translate_pending_path() -> PathBuf {
    state_dir().join("batch_translate_pending.json")
}

/// `state/scan_smart_rules.yaml` — security scan smart triage rules.
pub fn security_scan_smart_rules_path() -> PathBuf {
    state_dir().join("scan_smart_rules.yaml")
}

// ═══════════════════════════════════════════════════════════════════
//  Hub paths  (hub/)
// ═══════════════════════════════════════════════════════════════════

/// `hub/skills/` — the central skill hub (symlinks).
pub fn hub_skills_dir() -> PathBuf {
    hub_root().join("skills")
}

/// `hub/repos/` — cached cloned repositories.
pub fn repos_cache_dir() -> PathBuf {
    hub_root().join("repos")
}

/// `hub/publish/<repo>` — publish staging area.
pub fn publish_cache_dir(repo_name: &str) -> PathBuf {
    hub_root().join("publish").join(repo_name)
}

/// `hub/local/` — user-authored local skills.
pub fn local_skills_dir() -> PathBuf {
    hub_root().join("local")
}

/// `hub/lock.json` — installation lockfile.
pub fn lockfile_path() -> PathBuf {
    hub_root().join("lock.json")
}



// ═══════════════════════════════════════════════════════════════════
//  Legacy migration
// ═══════════════════════════════════════════════════════════════════

/// Migrate from the v1 flat layout to the v2 categorised layout.
///
/// Call this once at app startup. Idempotent — only moves files that
/// still exist at old locations and don't yet exist at new locations.
pub fn migrate_legacy_paths() {
    if MIGRATION_DONE.get().is_some() {
        return;
    }

    let root = data_root();

    // Ensure target directories exist
    let _ = std::fs::create_dir_all(config_dir());
    let _ = std::fs::create_dir_all(db_dir());
    let _ = std::fs::create_dir_all(logs_dir());
    let _ = std::fs::create_dir_all(state_dir());
    let _ = std::fs::create_dir_all(hub_root());

    // ── Config files ──
    migrate_file(&root.join("ai_config.json"), &ai_config_path());
    migrate_file(&root.join("acp_config.json"), &acp_config_path());
    migrate_file(&root.join("proxy.json"), &proxy_config_path());
    migrate_file(&root.join("profiles.toml"), &profiles_config_path());
    migrate_file(
        &root.join("security_scan_policy.yaml"),
        &security_scan_policy_path(),
    );

    // ── Databases ──
    migrate_file(&root.join("marketplace.db"), &marketplace_db_path());
    // Also migrate WAL/SHM files for SQLite
    migrate_file(
        &root.join("marketplace.db-wal"),
        &db_dir().join("marketplace.db-wal"),
    );
    migrate_file(
        &root.join("marketplace.db-shm"),
        &db_dir().join("marketplace.db-shm"),
    );
    migrate_file(&root.join("translation_cache.db"), &translation_db_path());
    migrate_file(
        &root.join("translation_cache.db-wal"),
        &db_dir().join("translation.db-wal"),
    );
    migrate_file(
        &root.join("translation_cache.db-shm"),
        &db_dir().join("translation.db-shm"),
    );
    migrate_file(&root.join("security_scan.db"), &security_scan_db_path());
    migrate_file(
        &root.join("security_scan.db-wal"),
        &db_dir().join("security.db-wal"),
    );
    migrate_file(
        &root.join("security_scan.db-shm"),
        &db_dir().join("security.db-shm"),
    );

    // ── Logs ──
    migrate_file(&root.join("security_scan.log"), &security_scan_log_path());
    migrate_dir(&root.join("security_scan_logs"), &security_scan_logs_dir());

    // ── State ──
    migrate_file(&root.join("patrol.json"), &patrol_state_path());
    migrate_file(&root.join("projects.json"), &projects_manifest_path());
    migrate_dir(&root.join("projects"), &state_dir().join("projects"));
    migrate_file(&root.join("groups.json"), &groups_path());
    migrate_file(&root.join("packs.json"), &packs_path());
    migrate_file(&root.join("repo_history.json"), &repo_history_path());
    migrate_file(&root.join("mymemory_usage.json"), &mymemory_usage_path());
    migrate_file(&root.join(".mymemory_de"), &mymemory_disabled_path());
    migrate_file(
        &root.join("batch_translate_pending.json"),
        &batch_translate_pending_path(),
    );
    migrate_file(
        &root.join("security_scan_smart_rules.yaml"),
        &security_scan_smart_rules_path(),
    );

    // ── Hub (from .agents/ → hub/) ──
    let legacy_hub = root.join(".agents");
    if legacy_hub.is_dir() {
        migrate_dir(&legacy_hub.join("skills"), &hub_skills_dir());
        migrate_dir(&legacy_hub.join("skills-local"), &local_skills_dir());
        migrate_dir(&legacy_hub.join(".repos"), &repos_cache_dir());
        migrate_dir(
            &legacy_hub.join(".publish-repos"),
            &hub_root().join("publish"),
        );
        migrate_file(&legacy_hub.join(".skill-lock.json"), &lockfile_path());

        // Clean up empty legacy hub
        let _ = remove_dir_if_empty(&legacy_hub);
    }



    // ── Clean up legacy files ──
    let _ = std::fs::remove_file(root.join("security_scan_cache.json"));

    let _ = MIGRATION_DONE.set(());

    tracing::info!("Storage path migration check completed");
}

/// Move a single file from old to new location.
fn migrate_file(old: &Path, new: &Path) {
    if old.exists() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(old, new) {
            Ok(()) => tracing::info!("Migrated {:?} → {:?}", old, new),
            Err(e) => tracing::warn!("Failed to migrate {:?} → {:?}: {}", old, new, e),
        }
    }
}

/// Move/rename a directory from old to new location.
fn migrate_dir(old: &Path, new: &Path) {
    if old.is_dir() && !new.exists() {
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::rename(old, new) {
            Ok(()) => tracing::info!("Migrated dir {:?} → {:?}", old, new),
            Err(e) => tracing::warn!("Failed to migrate dir {:?} → {:?}: {}", old, new, e),
        }
    }
}

/// Remove a directory only if it's empty.
fn remove_dir_if_empty(dir: &Path) -> std::io::Result<()> {
    if let Ok(mut entries) = std::fs::read_dir(dir) {
        if entries.next().is_none() {
            std::fs::remove_dir(dir)?;
            tracing::info!("Removed empty legacy dir {:?}", dir);
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════
//  Filesystem utilities
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
            retry_io(remove_op)
                .with_context(|| format!("Failed to remove symlink: {:?}", path))?;
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
                junction::delete(path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
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

// ═══════════════════════════════════════════════════════════════════
//  Helper
// ═══════════════════════════════════════════════════════════════════

/// Expand a leading `~/` or `~\` to the real home directory.
fn shellexpand_home(path: &str) -> String {
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    if let Some(rest) = rest {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(rest).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}
