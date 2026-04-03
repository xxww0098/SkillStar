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
//!     ├── setup-hooks/        # ACP-generated build scripts
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

/// `hub/setup-hooks/` — ACP-generated build scripts.
pub fn setup_hooks_dir() -> PathBuf {
    hub_root().join("setup-hooks")
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

    // ── Setup hooks (from data_root/setup-hooks → hub/setup-hooks) ──
    migrate_dir(&root.join("setup-hooks"), &setup_hooks_dir());

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

/// Cross-platform symlink removal.
///
/// On Windows, directory symlinks (created via `symlink_dir`) must be removed
/// with `remove_dir()`, not `remove_file()` — the latter returns
/// `ERROR_ACCESS_DENIED`.  This utility inspects the metadata and dispatches
/// correctly on every platform.
pub fn remove_symlink(path: &Path) -> anyhow::Result<()> {
    let meta = path
        .symlink_metadata()
        .with_context(|| format!("Failed to read symlink metadata: {:?}", path))?;

    if !meta.is_symlink() {
        anyhow::bail!("Not a symlink: {:?}", path);
    }

    #[cfg(unix)]
    std::fs::remove_file(path).with_context(|| format!("Failed to remove symlink: {:?}", path))?;

    #[cfg(windows)]
    {
        // Directory symlinks must use remove_dir; file symlinks use remove_file.
        if meta.is_dir() {
            std::fs::remove_dir(path)
        } else {
            std::fs::remove_file(path)
        }
        .with_context(|| format!("Failed to remove symlink: {:?}", path))?;
    }

    Ok(())
}

/// Check if the current platform supports symlink creation.
///
/// On Windows, returns true if EITHER symlinks or junction points work.
/// On Unix, always returns true.
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

/// Check if two paths are on the same Windows drive letter.
/// Junction points only work within the same drive.
#[cfg(windows)]
fn same_drive(a: &Path, b: &Path) -> bool {
    use std::path::Component;
    a.components().next().map_or(false, |ac| {
        b.components().next().map_or(false, |bc| ac == bc)
    })
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
