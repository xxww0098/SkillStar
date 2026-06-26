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

use std::path::PathBuf;

/// App root — all SkillStar data lives under here.
///
/// Default: `~/.skillstar/` (all platforms)
/// Override: `SKILLSTAR_DATA_DIR`
pub fn data_root() -> PathBuf {
    if let Ok(dir) = std::env::var("SKILLSTAR_DATA_DIR") {
        let expanded = shellexpand_home(&dir);
        return PathBuf::from(expanded);
    }
    #[cfg(test)]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".skillstar");
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
    #[cfg(test)]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home);
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

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

/// `config/ssh_hosts.toml` — SSH remote host definitions (non-sensitive metadata only;
/// passphrases/passwords live in the system keyring, keyed by host id).
pub fn ssh_hosts_config_path() -> PathBuf {
    config_dir().join("ssh_hosts.toml")
}

/// `config/ssh_known_hosts.json` — accepted SSH host-key fingerprints (TOFU store).
pub fn ssh_known_hosts_path() -> PathBuf {
    config_dir().join("ssh_known_hosts.json")
}

/// `config/s3_targets.toml` — S3 cloud sync target definitions (non-sensitive
/// metadata only: endpoint, region, bucket, prefix, access key id;
/// `secret_access_key` lives in the system keyring, keyed by target id).
pub fn s3_targets_config_path() -> PathBuf {
    config_dir().join("s3_targets.toml")
}

/// `state/sync_device.json` — this device's identity (hostname + suffix) so
/// pushed manifests can record their origin.
pub fn sync_device_id_path() -> PathBuf {
    state_dir().join("sync_device.json")
}

/// `db/marketplace.db` — local-first marketplace snapshot DB.
pub fn marketplace_db_path() -> PathBuf {
    db_dir().join("marketplace.db")
}

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

/// Expand a leading `~/` or `~\` to the real home directory.
pub(crate) fn shellexpand_home(path: &str) -> String {
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    if let Some(rest) = rest {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(rest).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{data_root, home_dir, hub_root, shellexpand_home};
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
