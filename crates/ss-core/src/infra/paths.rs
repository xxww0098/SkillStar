//! Centralised path resolution for all SkillStar storage locations.
//!
//! Every module that needs a filesystem path **must** go through this module
//! instead of calling `dirs::data_dir()` / `dirs::home_dir()` directly.
//!
//! ## Directory structure (v4, D-113)
//!
//! Classification rules and the old→new mapping live in
//! `docs/storage-layout.md` (the layout SSOT). Summary:
//!
//! ```text
//! ~/.skillstar/               # data_root()
//! ├── config/                 # User-editable declarative settings
//! ├── data/                   # Durable domain data & unified management tree
//! │   ├── skills/installed/   # Unified canonical skill entities (no local/ double-hop)
//! │   ├── skills/library.json # Unified skill library & deployment ledger
//! │   ├── skills/backups/     # Rolling skill backup snapshots
//! │   ├── skills/.skill-lock.json
//! │   ├── skills/{groups,favorites,repo_history}.json
//! │   ├── projects/           # Registered project manifests
//! │   ├── channels/           # Shared-channel descriptors & subscriptions
//! │   └── instances/          # Desktop multi-instance profiles + manifest
//! ├── secrets/                # Credentials & token-bearing stores (0700)
//! │   ├── accounts/           # Account records & encrypted tokens
//! │   └── github/             # GitHub user login credentials
//! ├── cache/                  # Rebuildable derived data
//! │   ├── marketplace/        # Marketplace snapshot SQLite + description cache
//! │   ├── skill-imports/      # Git sparse import checkouts
//! │   ├── github-trees/       # GitHub REST ETag response cache
//! │   ├── skill-publish/      # Publish staging cache
//! │   ├── avatars/            # Cached repository avatars
//! │   └── translations/       # Translation entries cache
//! ├── state/                  # Rebuildable runtime projections across restarts
//! ├── runtime/locks/          # Cross-process locks (never config/ or state/)
//! └── logs/                   # Runtime and per-run logs
//! ```
//!
//! ## Environment variable overrides
//!
//! | Variable | Default | Description |
//! |---|---|---|
//! | `SKILLSTAR_DATA_DIR` | `~/.skillstar` | App config & metadata root; also re-roots [`agents_skills_root`] and [`skill_lock_path`] |
//! | `SKILLSTAR_HUB_DIR` | *(unset)* | Test-sandbox re-root for the legacy hub and canonical skills root; when set, local skills stay at `<hub>/local`. Never set in production |
//! | `SKILLSTAR_TOOL_SYNC_HOME` | *(real home)* | Sandbox root for external tool-config paths |
//!
//! Setting these variables during development keeps dev data completely
//! separate from the production (installed) app.

use std::path::PathBuf;

/// App root — all SkillStar data lives under here.
///
/// Default: `~/.skillstar/` (all platforms)
/// Override: `SKILLSTAR_DATA_DIR`
pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_DATA_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    home_dir().join(".skillstar")
}

/// Hub root — skills, repo cache, lockfile, publish cache live here.
///
/// Default: `~/.skillstar/hub`
/// Override: `SKILLSTAR_HUB_DIR`
pub fn hub_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    data_root().join("hub")
}

/// Canonical installed-skills root: `~/.skillstar/data/skills/installed`.
///
/// Skill folders live here as real directory copies. Agent directories get a
/// link only when the user deploys to that Agent. `~/.agents/skills` is an
/// Agent directory some tools read directly; SkillStar does not store its
/// copies there (D-100). `SKILLSTAR_HUB_DIR` still re-roots this to
/// `<hub>/skills` for test sandboxes. `SKILLSTAR_DATA_DIR` moves it with the
/// rest of the data root.
pub fn agents_skills_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded).join("skills");
    }
    data_dir().join("skills").join("installed")
}

/// One canonical skill folder: `<agents_skills_root>/<name>`.
pub fn agents_skill_dir(name: &str) -> PathBuf {
    agents_skills_root().join(name)
}

/// Install lock (vercel `.skill-lock.json` v3) beside the canonical skills.
///
/// Production: `~/.skillstar/data/skills/.skill-lock.json`. `SKILLSTAR_HUB_DIR`
/// keeps the sandbox lock at `<hub>/.skill-lock.json`.
pub fn skill_lock_path() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded).join(".skill-lock.json");
    }
    data_dir().join("skills").join(".skill-lock.json")
}

/// Pre-D-100 canonical directory. Startup migration moves SkillStar copies out
/// of here. Not a write target.
pub fn legacy_agents_skills_root() -> PathBuf {
    home_dir().join(".agents").join("skills")
}

/// Lock files the pre-D-100 resolver would have read, most preferred first.
pub fn legacy_skill_lock_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME")
        && !xdg.trim().is_empty()
    {
        paths.push(PathBuf::from(xdg).join("skills").join(".skill-lock.json"));
    }
    paths.push(home_dir().join(".agents").join(".skill-lock.json"));
    paths
}

/// User home directory (used for agent profile dirs like `~/.claude/skills`).
///
/// Honors the platform home env var before falling back to `dirs`. This is not
/// merely a test affordance: `dirs::home_dir()` reads `$HOME` on Unix but
/// resolves the Windows profile via `SHGetKnownFolderPath`, ignoring
/// `USERPROFILE`. Without this branch a test that sandboxes `USERPROFILE`
/// silently deploys into the runner's real profile on Windows — exactly the
/// sandbox escape ci.yml failure lesson 2 warns about (observed as
/// `batch_global_deploy_honors_explicit_copy_mode` failing on Windows CI).
pub fn home_dir() -> PathBuf {
    if let Some(home) = home_env_override() {
        return home;
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Env var that re-roots every external tool-config path (`~/.codex`,
/// `~/.claude`, `~/.grok`, `~/.config/opencode`, …) under a sandbox directory.
///
/// When set to a non-empty path, tool-config path resolution happens *inside*
/// that directory instead of the user's real home. Tests MUST set this so the
/// suite never overwrites a developer's live tool configuration; it also lets
/// advanced users sandbox tool sync. Crates that need the raw name import it
/// from here instead of redeclaring a private copy.
pub const TOOL_SYNC_HOME_ENV: &str = "SKILLSTAR_TOOL_SYNC_HOME";

/// The [`TOOL_SYNC_HOME_ENV`] sandbox root when set to a non-empty path.
pub fn tool_sync_home_override() -> Option<PathBuf> {
    std::env::var_os(TOOL_SYNC_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// `$HOME` on Unix, `%USERPROFILE%` on Windows — the canonical home env var
/// for the platform. Empty values are ignored so a blank var never redirects
/// paths to the filesystem root.
fn home_env_override() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

// ── v3 top-level categories (D-087; docs/storage-layout.md) ─────────────

/// `~/.skillstar/data/` — durable domain data & user content. Never
/// auto-cleaned; must be backed up.
pub fn data_dir() -> PathBuf {
    data_root().join("data")
}

/// `~/.skillstar/secrets/` — credentials and token-bearing stores. The
/// storage migration creates it with 0700 on Unix; individual sealed files
/// keep their own 0600 modes.
pub fn secrets_dir() -> PathBuf {
    data_root().join("secrets")
}

/// `~/.skillstar/cache/` — rebuildable derived data; safe to delete whole.
pub fn cache_dir() -> PathBuf {
    data_root().join("cache")
}

/// Rebuildable partial Git checkouts for skill imports.
pub fn skill_import_cache_dir() -> PathBuf {
    cache_dir().join("skill-imports")
}

/// Kept outside the cache so cleanup cannot replace a live lock's inode.
pub fn skill_import_locks_dir() -> PathBuf {
    runtime_locks_dir().join("skill-imports")
}

/// `~/.skillstar/runtime/` — cross-process coordination files (locks).
/// Never backed up; not cleaned while a live process holds a lock.
pub fn runtime_dir() -> PathBuf {
    data_root().join("runtime")
}

/// `~/.skillstar/runtime/locks/` — cross-process lock files.
pub fn runtime_locks_dir() -> PathBuf {
    runtime_dir().join("locks")
}

/// `~/.skillstar/config/` — user-editable configuration files.
pub fn config_dir() -> PathBuf {
    data_root().join("config")
}

/// `~/.skillstar/logs/` — runtime and per-run logs.
pub fn logs_dir() -> PathBuf {
    data_root().join("logs")
}

/// `~/.skillstar/state/` — rebuildable runtime state & metadata.
pub fn state_dir() -> PathBuf {
    data_root().join("state")
}

/// `data/instances/` — isolated Chromium/Electron profiles for desktop multi-instance.
pub fn instances_dir() -> PathBuf {
    data_dir().join("instances")
}

/// `data/instances/app_instances.json` — desktop multi-instance registry.
pub fn app_instances_config_path() -> PathBuf {
    instances_dir().join("app_instances.json")
}
/// `config/ai.json` — AI provider configuration.
pub fn ai_config_path() -> PathBuf {
    config_dir().join("ai.json")
}

/// `config/proxy.json` — proxy configuration.
pub fn proxy_config_path() -> PathBuf {
    config_dir().join("proxy.json")
}

/// `config/github_mirror.json` — GitHub mirror/accelerator configuration.
pub fn github_mirror_config_path() -> PathBuf {
    config_dir().join("github_mirror.json")
}

/// `config/skill_updates.json` — automatic vs manual Skill update preference.
pub fn skill_updates_config_path() -> PathBuf {
    config_dir().join("skill_updates.json")
}

/// `config/translation.json` — engine, themes, and the LLM endpoint the user typed.
pub fn translation_config_path() -> PathBuf {
    config_dir().join("translation.json")
}

/// `secrets/translation/api_key` — leftover path. LLM translation reads the
/// OpenCode Go, Ollama, or Command Code key from Accounts instead.
pub fn translation_api_key_path() -> PathBuf {
    secrets_dir().join("translation").join("api_key")
}

/// `cache/translations/entries.json` — translations rebuilt from source text.
pub fn translation_cache_path() -> PathBuf {
    cache_dir().join("translations").join("entries.json")
}

/// `state/skill_auto_update.json` — when the background Skill update monitor
/// last ran. Rebuildable scheduling state, not user configuration.
pub fn skill_auto_update_state_path() -> PathBuf {
    state_dir().join("skill_auto_update.json")
}

/// `state/skills/github_api_cooldown.json` — when the GitHub REST rate limit
/// that update checks hit resets. Until then checks skip the API entirely.
pub fn github_api_cooldown_path() -> PathBuf {
    state_dir().join("skills").join("github_api_cooldown.json")
}

/// `state/app/release_check.json` — outcome of the last SkillStar release
/// check. Written by the manual check and the daily background wake; the
/// About section reads it so it can show a result without touching the
/// network.
pub fn app_release_check_state_path() -> PathBuf {
    state_dir().join("app").join("release_check.json")
}

/// `data/skills/install_baselines.json` — legacy v3 baseline file path (migrated into `library.json`).
pub fn skill_install_baselines_path() -> PathBuf {
    skills_data_dir().join("install_baselines.json")
}

/// `~/.skillstar/data/skills` — unified skills data & management tree root.
pub fn skills_data_dir() -> PathBuf {
    data_dir().join("skills")
}

/// Base directory honoring `SKILLSTAR_HUB_DIR` for sandboxed skill metadata,
/// or [`skills_data_dir`] in production.
fn sandbox_or_skills_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    skills_data_dir()
}

/// `data/skills/library.json` — unified skill management ledger.
pub fn skills_library_path() -> PathBuf {
    sandbox_or_skills_data_dir().join("library.json")
}

/// `data/skills/backups/` — rolling skill backup snapshots.
pub fn skills_backups_dir() -> PathBuf {
    sandbox_or_skills_data_dir().join("backups")
}

/// `data/skills/.preserve-staging/` — internal two-phase staging for channel removal conversion.
pub fn skills_preserve_staging_dir() -> PathBuf {
    sandbox_or_skills_data_dir().join(".preserve-staging")
}

/// `data/skills/favorites.json` — repository sources the user favorited
/// on this machine. Migrated from `repo-favorites.json`.
pub fn repo_favorites_path() -> PathBuf {
    skills_data_dir().join("favorites.json")
}

/// `data/skills/repo-favorites.json` — legacy v3 favorites path.
pub(crate) fn legacy_repo_favorites_path() -> PathBuf {
    skills_data_dir().join("repo-favorites.json")
}

/// `config/profiles.toml` — agent profile definitions.
pub fn profiles_config_path() -> PathBuf {
    config_dir().join("profiles.toml")
}

/// `secrets/ssh/credentials.json` — encrypted SSH passphrases/passwords.
/// Tokens are AES-256-GCM sealed JSON on disk (mode 0600). Not the OS keychain.
pub fn ssh_credentials_path() -> PathBuf {
    secrets_dir().join("ssh").join("credentials.json")
}

/// `state/sync_device.json` — this device's identity (hostname + suffix) so
/// `cache/marketplace/` — rebuildable marketplace snapshot data.
pub fn marketplace_cache_dir() -> PathBuf {
    cache_dir().join("marketplace")
}

/// `cache/marketplace/marketplace.db` — local-first marketplace snapshot DB.
pub fn marketplace_db_path() -> PathBuf {
    marketplace_cache_dir().join("marketplace.db")
}

/// `cache/marketplace/description_cache.json` — cached marketplace descriptions.
pub fn marketplace_description_cache_path() -> PathBuf {
    marketplace_cache_dir().join("description_cache.json")
}

/// `cache/avatars/` — cached GitHub organization / owner avatars.
pub fn avatars_cache_dir() -> PathBuf {
    cache_dir().join("avatars")
}

/// `cache/github-trees/` — conditional ETag cache for GitHub REST update checks.
pub fn github_trees_cache_dir() -> PathBuf {
    cache_dir().join("github-trees")
}

/// `state/patrol/status.json` — patrol background-run state.
pub fn patrol_state_path() -> PathBuf {
    state_dir().join("patrol").join("status.json")
}

/// `state/team.json` — local team intelligence (learnings, usage, recall, friction).
/// Rebuildable. Distinct from the deleted `learning/` tutorial tree (D-053).
pub fn team_store_path() -> PathBuf {
    state_dir().join("team.json")
}

/// `state/github_mirror_health.json` — GitHub accelerator circuit-breaker state.
pub fn github_mirror_health_path() -> PathBuf {
    state_dir().join("github_mirror_health.json")
}

/// `secrets/github/auth.json` — encrypted GitHub access/refresh credentials.
/// Tokens are AES-256-GCM sealed JSON on disk (mode 0600). Not the OS keychain.
pub fn github_auth_path() -> PathBuf {
    secrets_dir().join("github").join("auth.json")
}

// ── Usage account storage & locks (v3) ──────────────────────────────────

/// `secrets/accounts/usage/` — Usage subscription store. Subscriptions carry
/// encrypted tokens, so the whole store is treated as secrets (D-087 Phase 1).
pub fn usage_store_dir() -> PathBuf {
    secrets_dir().join("accounts").join("usage")
}

/// `runtime/locks/accounts/storage.lock` — usage storage write guard.
pub fn usage_storage_lock_path() -> PathBuf {
    runtime_locks_dir().join("accounts").join("storage.lock")
}

/// `runtime/locks/accounts/` — per-catalog usage refresh locks
/// (`catalog-<id>.lock`).
pub fn usage_refresh_locks_dir() -> PathBuf {
    runtime_locks_dir().join("accounts")
}

/// `secrets/accounts/cli/` — root of the CLI credential custody snapshots
/// (one `<catalog>/<subscription_id>.json` per snapshot, D-077).
pub fn cli_custody_root() -> PathBuf {
    secrets_dir().join("accounts").join("cli")
}

/// `cache/sessions/index.json` — Agent session parsing checkpoint index.
pub fn sessions_index_path() -> PathBuf {
    cache_dir().join("sessions").join("index.json")
}

/// `runtime/locks/skills/update.lock` — global skill update transaction lock.
pub fn skill_update_lock_path() -> PathBuf {
    runtime_locks_dir().join("skills").join("update.lock")
}

/// `runtime/locks/skills/skill-lock.lock` — serializes `.skill-lock.json`
/// read-modify-write cycles across processes.
pub fn skill_lock_write_lock_path() -> PathBuf {
    runtime_locks_dir().join("skills").join("skill-lock.lock")
}

/// `state/skill_updates.json` — persisted update-available and sibling-skill projection.
pub fn skill_updates_state_path() -> PathBuf {
    state_dir().join("skill_updates.json")
}

/// `data/projects/` — registered project manifests root (migrated from `state/projects`).
pub fn projects_dir() -> PathBuf {
    data_dir().join("projects")
}

/// `data/projects/projects.json` — registered projects manifest.
pub fn projects_manifest_path() -> PathBuf {
    projects_dir().join("projects.json")
}

/// `data/projects/<name>` — per-project detail directory.
pub fn project_detail_dir(name: &str) -> PathBuf {
    projects_dir().join(name)
}

/// `data/channels/` — shared-channel local registry & subscriptions.
pub fn channels_data_dir() -> PathBuf {
    data_dir().join("channels")
}

/// `data/channels/shared_channels.json` — shared channel descriptor registry.
pub fn shared_channels_registry_path() -> PathBuf {
    channels_data_dir().join("shared_channels.json")
}

/// `data/channels/subscriptions.json` — shared channel subscription store.
pub fn channel_subscriptions_path() -> PathBuf {
    channels_data_dir().join("subscriptions.json")
}

/// `runtime/locks/channels/` — cross-process locks for shared-channel registry & auto-update.
pub fn channels_locks_dir() -> PathBuf {
    runtime_locks_dir().join("channels")
}

/// `data/skills/groups.json` — skill groups (migrated from `state/groups.json`).
pub fn groups_path() -> PathBuf {
    skills_data_dir().join("groups.json")
}

/// `data/skills/repo_history.json` — repo import history (migrated from `state/repo_history.json`).
pub fn repo_history_path() -> PathBuf {
    skills_data_dir().join("repo_history.json")
}

/// Canonical installed-skills directory alias (`~/.skillstar/data/skills/installed`).
pub fn installed_skills_dir() -> PathBuf {
    agents_skills_root()
}

/// The canonical installed-skills root.
///
/// This IS [`agents_skills_root`] (`~/.skillstar/data/skills/installed` in
/// production). The name survives because deployment code predates the rename.
pub fn hub_skills_dir() -> PathBuf {
    agents_skills_root()
}

/// The pre-D-081 physical hub root (`~/.skillstar/hub`), used only by the
/// one-time legacy cleanup. Honors the same env overrides as [`hub_root`].
pub fn legacy_hub_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    data_root().join("hub")
}

/// `hub/repos/` — legacy cached cloned repositories (v4 migration/cleanup only).
pub fn repos_cache_dir() -> PathBuf {
    hub_root().join("repos")
}

/// `cache/skill-publish/` — root directory for temporary publish clones.
pub fn publish_cache_root() -> PathBuf {
    cache_dir().join("skill-publish")
}

/// `cache/skill-publish/<repo>` — publish staging area.
pub fn publish_cache_dir(repo_name: &str) -> PathBuf {
    publish_cache_root().join(repo_name)
}

/// Legacy v3 local-skills directory (`data/skills/local` or `<hub>/local`).
///
/// D-113 unified all skill entities under `data/skills/installed/`; this path
/// is kept for v3→v4 migration and backward-compatible sandbox probes.
pub fn legacy_local_skills_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded).join("local");
    }
    data_dir().join("skills").join("local")
}

/// Compatibility accessor for the canonical local-authoring root.
///
/// Under v4 (D-113), local skills live directly alongside remote skills inside
/// [`installed_skills_dir`] (`data/skills/installed/`), without a `local/` symlink
/// double-hop.
pub fn local_skills_dir() -> PathBuf {
    agents_skills_root()
}

/// `hub/lock.json` — installation lockfile.
pub fn lockfile_path() -> PathBuf {
    hub_root().join("lock.json")
}

/// Expand a leading `~/` or `~\` to the real home directory.
pub(crate) fn shellexpand_home(path: &str) -> String {
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    if let Some(rest) = rest {
        home_dir().join(rest).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        agents_skills_root, app_instances_config_path, data_root, github_auth_path, home_dir,
        hub_root, instances_dir, legacy_local_skills_dir, local_skills_dir, marketplace_db_path,
        patrol_state_path, sessions_index_path, shellexpand_home, skill_lock_path,
        skill_update_lock_path, skills_backups_dir, skills_library_path, ssh_credentials_path,
        usage_refresh_locks_dir, usage_storage_lock_path, usage_store_dir,
    };
    use tempfile::TempDir;

    #[test]
    fn data_root_honors_override() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        assert_eq!(data_root(), temp.path());
        assert_eq!(instances_dir(), temp.path().join("data/instances"));
        assert_eq!(
            app_instances_config_path(),
            temp.path().join("data/instances/app_instances.json")
        );
        assert_eq!(
            crate::infra::paths::team_store_path(),
            temp.path().join("state/team.json")
        );
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn hub_root_defaults_under_data_root() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::remove_var("SKILLSTAR_HUB_DIR");
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
        }
        assert_eq!(hub_root(), temp.path().join("hub"));
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn hub_root_honors_override() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
        }
        assert_eq!(hub_root(), temp.path());
        unsafe {
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }
    }

    #[test]
    fn v3_layout_categories_resolve_under_data_root() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }
        assert_eq!(
            marketplace_db_path(),
            temp.path().join("cache/marketplace/marketplace.db")
        );
        assert_eq!(
            sessions_index_path(),
            temp.path().join("cache/sessions/index.json")
        );
        assert_eq!(
            github_auth_path(),
            temp.path().join("secrets/github/auth.json")
        );
        assert_eq!(
            ssh_credentials_path(),
            temp.path().join("secrets/ssh/credentials.json")
        );
        assert_eq!(
            usage_store_dir(),
            temp.path().join("secrets/accounts/usage")
        );
        assert_eq!(
            usage_storage_lock_path(),
            temp.path().join("runtime/locks/accounts/storage.lock")
        );
        assert_eq!(
            usage_refresh_locks_dir(),
            temp.path().join("runtime/locks/accounts")
        );
        assert_eq!(
            skill_update_lock_path(),
            temp.path().join("runtime/locks/skills/update.lock")
        );
        assert_eq!(
            patrol_state_path(),
            temp.path().join("state/patrol/status.json")
        );
        assert_eq!(
            legacy_local_skills_dir(),
            temp.path().join("data/skills/local")
        );
        assert_eq!(
            local_skills_dir(),
            temp.path().join("data/skills/installed")
        );
        assert_eq!(
            agents_skills_root(),
            temp.path().join("data/skills/installed")
        );
        assert_eq!(
            skills_library_path(),
            temp.path().join("data/skills/library.json")
        );
        assert_eq!(
            skills_backups_dir(),
            temp.path().join("data/skills/backups")
        );
        assert_eq!(
            skill_lock_path(),
            temp.path().join("data/skills/.skill-lock.json")
        );
        assert_eq!(
            crate::infra::paths::repo_favorites_path(),
            temp.path().join("data/skills/favorites.json")
        );
        assert_eq!(
            crate::infra::paths::groups_path(),
            temp.path().join("data/skills/groups.json")
        );
        assert_eq!(
            crate::infra::paths::repo_history_path(),
            temp.path().join("data/skills/repo_history.json")
        );
        assert_eq!(
            crate::infra::paths::projects_manifest_path(),
            temp.path().join("data/projects/projects.json")
        );
        assert_eq!(
            crate::infra::paths::shared_channels_registry_path(),
            temp.path().join("data/channels/shared_channels.json")
        );
        assert_eq!(
            crate::infra::paths::channel_subscriptions_path(),
            temp.path().join("data/channels/subscriptions.json")
        );
        assert_eq!(
            crate::infra::paths::channels_locks_dir(),
            temp.path().join("runtime/locks/channels")
        );
        assert_eq!(
            crate::infra::paths::avatars_cache_dir(),
            temp.path().join("cache/avatars")
        );
        assert_eq!(
            crate::infra::paths::github_trees_cache_dir(),
            temp.path().join("cache/github-trees")
        );
        assert_eq!(
            crate::infra::paths::skill_updates_state_path(),
            temp.path().join("state/skill_updates.json")
        );
        unsafe {
            std::env::remove_var("SKILLSTAR_DATA_DIR");
        }
    }

    #[test]
    fn local_skills_dir_stays_in_hub_sandbox() {
        let _env_lock = crate::config::test_env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let temp = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("SKILLSTAR_HUB_DIR", temp.path());
        }
        assert_eq!(legacy_local_skills_dir(), temp.path().join("local"));
        assert_eq!(local_skills_dir(), temp.path().join("skills"));
        assert_eq!(skills_library_path(), temp.path().join("library.json"));
        assert_eq!(skills_backups_dir(), temp.path().join("backups"));
        unsafe {
            std::env::remove_var("SKILLSTAR_HUB_DIR");
        }
    }

    #[test]
    fn shellexpand_home_expands_tilde() {
        let home = home_dir();
        assert_eq!(
            shellexpand_home("~/foo"),
            home.join("foo").to_string_lossy()
        );
        assert_eq!(
            shellexpand_home("~\\foo"),
            home.join("foo").to_string_lossy()
        );
        assert_eq!(shellexpand_home("/absolute/path"), "/absolute/path");
        assert_eq!(shellexpand_home("relative/path"), "relative/path");
    }
}
