//! Centralised path resolution for all SkillStar storage locations.
//!
//! Every module that needs a filesystem path **must** go through this module
//! instead of calling `dirs::data_dir()` / `dirs::home_dir()` directly.
//!
//! ## Directory structure (v3, D-087)
//!
//! Classification rules and the old→new mapping live in
//! `docs/storage-layout.md` (the layout SSOT). Summary:
//!
//! ```text
//! ~/.skillstar/               # data_root()
//! ├── config/                 # User-editable declarative settings
//! ├── data/                   # Durable domain data & user content
//! │   ├── skills/installed/   # Installed skill copies (not ~/.agents/skills)
//! │   ├── skills/local/       # User-authored local skills
//! │   ├── skills/.skill-lock.json
//! │   └── instances/          # Desktop multi-instance profiles + manifest
//! ├── secrets/                # Credentials & token-bearing stores (0700)
//! │   ├── accounts/usage/     # Usage subscription store (encrypted tokens)
//! │   ├── accounts/cli/       # CLI credential custody snapshots
//! │   ├── github/             # GitHub user login credentials
//! │   └── ssh/                # Encrypted SSH credentials
//! ├── cache/                  # Rebuildable derived data
//! │   ├── marketplace/        # Marketplace snapshot SQLite
//! │   └── sessions/           # Session checkpoint index
//! ├── state/                  # Runtime state kept across restarts
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

/// `~/.skillstar/instances/<app>/<id>/` — one instance's user-data-dir.
///
/// `app` and `id` must be single path segments (no separators). Callers that
/// accept user input must validate before calling; this function does not
/// create the directory.
pub fn instance_profile_dir(app: &str, id: &str) -> PathBuf {
    instances_dir().join(app).join(id)
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

/// `data/skills/install_baselines.json` — content hash of every canonical
/// Skill as SkillStar last installed it. Not rebuildable: it is what proves a
/// Skill was not edited locally before an automatic update overwrites it.
pub fn skill_install_baselines_path() -> PathBuf {
    data_dir().join("skills").join("install_baselines.json")
}

/// `config/profiles.toml` — agent profile definitions.
pub fn profiles_config_path() -> PathBuf {
    config_dir().join("profiles.toml")
}

/// `config/ssh_hosts.toml` — SSH remote host definitions (non-sensitive metadata only;
/// passphrases/passwords live in secrets/ssh/credentials.json, keyed by host id).
pub fn ssh_hosts_config_path() -> PathBuf {
    config_dir().join("ssh_hosts.toml")
}

/// `secrets/ssh/credentials.json` — encrypted SSH passphrases/passwords.
/// Tokens are AES-256-GCM sealed JSON on disk (mode 0600). Not the OS keychain.
pub fn ssh_credentials_path() -> PathBuf {
    secrets_dir().join("ssh").join("credentials.json")
}

/// `config/ssh_known_hosts.json` — accepted SSH host-key fingerprints (TOFU store).
pub fn ssh_known_hosts_path() -> PathBuf {
    config_dir().join("ssh_known_hosts.json")
}

/// `config/s3_targets.toml` — S3 cloud sync target definitions (non-sensitive
/// metadata only: endpoint, region, bucket, prefix, access key id;
/// `config/antigravity_oauth.json` — Antigravity Google OAuth client credentials
/// (not shipped in the repo; copy from `.env.example` or Antigravity IDE).
pub fn antigravity_oauth_config_path() -> PathBuf {
    config_dir().join("antigravity_oauth.json")
}

/// `config/oauth_clients.json` — per-provider OAuth client id/secret overrides
/// (codex / xai / opencode). Optional; built-in defaults are used when
/// absent. Shape: `{ "codex": { "client_id": "...", "client_secret": "..." }, ... }`.
pub fn oauth_clients_config_path() -> PathBuf {
    config_dir().join("oauth_clients.json")
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

/// `state/usage/zcode-device-mid` — this machine's fallback `X-Device-Mid`
/// for zcode.z.ai quota calls when the official ZCode client's `deviceMid`
/// is absent. One UUID per machine, made once and never rotated.
pub fn zcode_device_mid_path() -> PathBuf {
    state_dir().join("usage").join("zcode-device-mid")
}

/// `secrets/accounts/cli/` — root of the CLI credential custody snapshots
/// (one `<catalog>/<subscription_id>.json` per snapshot, D-077).
pub fn cli_custody_root() -> PathBuf {
    secrets_dir().join("accounts").join("cli")
}

/// `secrets/accounts/cli/<catalog>/` — custody snapshots for one catalog.
/// Existing legacy catalogs stay in place: external CLI links reference them.
/// Metadata errors fail closed to the old path instead of selecting new data.
pub fn cli_custody_dir(catalog_id: &str) -> PathBuf {
    let legacy = data_root().join("accounts").join(catalog_id);
    if !matches!(legacy.symlink_metadata(), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    {
        return legacy;
    }
    cli_custody_root().join(catalog_id)
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

/// `state/repo_history.json` — repo import history.
pub fn repo_history_path() -> PathBuf {
    state_dir().join("repo_history.json")
}

/// The canonical installed-skills root.
///
/// This IS [`agents_skills_root`] (`~/.skillstar/data/skills/installed` in
/// production). The name survives because deployment and local-authoring code
/// predates the rename. The pre-D-081 physical directory lives under
/// `legacy_hub_root`; the pre-D-100 directory is [`legacy_agents_skills_root`].
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

/// `hub/repos/` — cached cloned repositories.
pub fn repos_cache_dir() -> PathBuf {
    hub_root().join("repos")
}

/// `hub/publish/<repo>` — publish staging area.
pub fn publish_cache_dir(repo_name: &str) -> PathBuf {
    hub_root().join("publish").join(repo_name)
}

/// `data/skills/local/` — user-authored local skills (v3; was `hub/local`).
///
/// Under a `SKILLSTAR_HUB_DIR` sandbox the legacy `<hub>/local` location
/// still applies so hub-rooted test sandboxes keep working; production never
/// sets that variable (D-087).
pub fn local_skills_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_HUB_DIR")
        && !dir.trim().is_empty()
    {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded).join("local");
    }
    data_dir().join("skills").join("local")
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
        agents_skills_root, app_instances_config_path, cli_custody_dir, data_root,
        github_auth_path, home_dir, hub_root, instance_profile_dir, instances_dir,
        local_skills_dir, marketplace_db_path, patrol_state_path, sessions_index_path,
        shellexpand_home, skill_lock_path, skill_update_lock_path, ssh_credentials_path,
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
            instance_profile_dir("cursor", "work"),
            temp.path().join("data/instances/cursor/work")
        );
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
            cli_custody_dir("codex"),
            temp.path().join("secrets/accounts/cli/codex")
        );
        assert_eq!(
            skill_update_lock_path(),
            temp.path().join("runtime/locks/skills/update.lock")
        );
        assert_eq!(
            patrol_state_path(),
            temp.path().join("state/patrol/status.json")
        );
        assert_eq!(local_skills_dir(), temp.path().join("data/skills/local"));
        // A live CLI may still link into the old catalog. Even a conflicting
        // new directory must not silently change the source of credentials.
        let legacy = temp.path().join("accounts/codex");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(temp.path().join("secrets/accounts/cli/codex")).unwrap();
        assert_eq!(cli_custody_dir("codex"), legacy);
        assert_eq!(
            agents_skills_root(),
            temp.path().join("data/skills/installed")
        );
        assert_eq!(
            skill_lock_path(),
            temp.path().join("data/skills/.skill-lock.json")
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
        assert_eq!(local_skills_dir(), temp.path().join("local"));
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
