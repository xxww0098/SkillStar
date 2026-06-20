//! Persistence for SSH hosts and their credentials.
//!
//! Two storage tiers mirror the existing config patterns:
//!
//! - **Host metadata** (`~/.skillstar/config/ssh_hosts.toml`) — a `Vec<SshHostDef>`
//!   containing only non-sensitive fields (display name, host, port, username,
//!   auth method, key *path*). Safe to back up.
//! - **Credentials** — passphrases and passwords go through the system keyring
//!   via the [`SecretStore`] trait. The production impl uses `keyring` v4;
//!   tests use [`MemSecretStore`].
//! - **Accepted host keys** (`~/.skillstar/config/ssh_known_hosts.json`) — the
//!   TOFU store written when the user confirms a server fingerprint.
//!
//! This mirrors `skillstar_projects::projects::agents::profile_storage`
//! (`TomlPrefsStore` + in-memory test double).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::types::{KnownHost, SshHostDef};

/// Service name used for every keyring entry. The account name is the host `id`.
const KEYRING_SERVICE: &str = "skillstar-ssh";

// ── Host metadata persistence ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HostsFile {
    #[serde(default)]
    hosts: Vec<SshHostDef>,
}

fn hosts_config_path() -> PathBuf {
    skillstar_core::infra::paths::ssh_hosts_config_path()
}

/// Load all SSH host definitions from disk. Missing/corrupt file → empty list.
pub fn load_hosts() -> Vec<SshHostDef> {
    let path = hosts_config_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: HostsFile = toml::from_str(&content).unwrap_or_default();
    file.hosts
}

/// Persist the full host list to disk (atomic write via tmp + rename).
pub fn save_hosts(hosts: &[SshHostDef]) -> Result<()> {
    let path = hosts_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create ssh_hosts config dir")?;
    }
    let file = HostsFile {
        hosts: hosts.to_vec(),
    };
    let content = toml::to_string_pretty(&file).context("serialize ssh_hosts.toml")?;

    // Atomic write: tmp file then rename, mirroring update_skill_content.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, content).context("write ssh_hosts.toml.tmp")?;
    std::fs::rename(&tmp, &path).context("rename ssh_hosts.toml")?;
    Ok(())
}

// ── Credential storage (keyring abstraction) ────────────────────────

/// Abstraction over secret storage so logic can be unit-tested without a
/// real OS keyring. Mirrors the `PrefsStore` pattern in `profile_storage.rs`.
pub trait SecretStore {
    fn get_secret(&self, host_id: &str) -> Result<Option<String>>;
    fn set_secret(&self, host_id: &str, value: &str) -> Result<()>;
    fn delete_secret(&self, host_id: &str) -> Result<()>;
}

/// Production secret store backed by the OS keyring.
///
/// On headless Linux without a Secret Service (D-Bus) the keyring calls will
/// error; callers should surface that to the UI rather than crashing.
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get_secret(&self, host_id: &str) -> Result<Option<String>> {
        match keyring::Entry::new(KEYRING_SERVICE, host_id) {
            Ok(entry) => match entry.get_password() {
                Ok(pw) => Ok(Some(pw)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(anyhow::anyhow!(err).context("keyring get_password")),
            },
            Err(err) => Err(anyhow::anyhow!(err).context("keyring entry new")),
        }
    }

    fn set_secret(&self, host_id: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, host_id)
            .map_err(|e| anyhow::anyhow!(e).context("keyring entry new"))?;
        entry
            .set_password(value)
            .map_err(|e| anyhow::anyhow!(e).context("keyring set_password"))?;
        Ok(())
    }

    fn delete_secret(&self, host_id: &str) -> Result<()> {
        match keyring::Entry::new(KEYRING_SERVICE, host_id) {
            Ok(entry) => match entry.delete_credential() {
                Ok(()) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(err) => Err(anyhow::anyhow!(err).context("keyring delete_credential")),
            },
            Err(err) => Err(anyhow::anyhow!(err).context("keyring entry new")),
        }
    }
}

/// In-memory secret store for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemSecretStore {
    map: std::cell::RefCell<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl MemSecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl SecretStore for MemSecretStore {
    fn get_secret(&self, host_id: &str) -> Result<Option<String>> {
        Ok(self.map.borrow().get(host_id).cloned())
    }
    fn set_secret(&self, host_id: &str, value: &str) -> Result<()> {
        self.map.borrow_mut().insert(host_id.to_string(), value.into());
        Ok(())
    }
    fn delete_secret(&self, host_id: &str) -> Result<()> {
        self.map.borrow_mut().remove(host_id);
        Ok(())
    }
}

// ── High-level host CRUD ────────────────────────────────────────────

/// Convenience façade combining host metadata + credential storage.
///
/// `HostsStore` is constructed with a [`SecretStore`] impl; production code
/// uses [`KeyringSecretStore`], tests pass a [`MemSecretStore`].
pub struct HostsStore<S: SecretStore> {
    secrets: S,
}

impl<S: SecretStore> HostsStore<S> {
    pub fn new(secrets: S) -> Self {
        Self { secrets }
    }

    /// Generate a host id unique against the current on-disk list.
    pub fn fresh_id() -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("ssh_{now_ms}")
    }

    /// Insert a new host. If `def.id` is empty it is auto-generated and the
    /// populated def is returned. `credential` (passphrase or password) is
    /// written to the secret store when non-empty.
    pub fn add(&self, mut def: SshHostDef, credential: Option<&str>) -> Result<SshHostDef> {
        if def.id.trim().is_empty() {
            def.id = Self::fresh_id();
        }
        if let Some(pw) = credential.filter(|s| !s.is_empty()) {
            self.secrets.set_secret(&def.id, pw)?;
        }
        let mut hosts = load_hosts();
        if hosts.iter().any(|h| h.id == def.id) {
            anyhow::bail!("SSH host id '{}' already exists", def.id);
        }
        hosts.push(def.clone());
        save_hosts(&hosts)?;
        Ok(def)
    }

    /// Replace a host by id. If `credential` is `Some`, the stored secret is
    /// updated (empty string clears it); `None` leaves the existing secret.
    pub fn update(&self, id: &str, def: SshHostDef, credential: Option<&str>) -> Result<()> {
        let mut hosts = load_hosts();
        let target = hosts
            .iter_mut()
            .find(|h| h.id == id)
            .ok_or_else(|| anyhow::anyhow!("SSH host '{}' not found", id))?;
        // If the id changed, move the secret so credentials follow the host.
        if def.id != *id {
            if let Some(pw) = self.secrets.get_secret(id)? {
                self.secrets.set_secret(&def.id, &pw)?;
                self.secrets.delete_secret(id)?;
            }
        }
        if let Some(pw) = credential {
            if pw.is_empty() {
                self.secrets.delete_secret(&def.id)?;
            } else {
                self.secrets.set_secret(&def.id, pw)?;
            }
        }
        *target = def;
        save_hosts(&hosts)
    }

    /// Remove a host by id and delete its secret.
    pub fn remove(&self, id: &str) -> Result<()> {
        let mut hosts = load_hosts();
        let before = hosts.len();
        hosts.retain(|h| h.id != id);
        if hosts.len() == before {
            anyhow::bail!("SSH host '{}' not found", id);
        }
        save_hosts(&hosts)?;
        // Best-effort secret cleanup — never fail the delete because the
        // keyring is unavailable.
        let _ = self.secrets.delete_secret(id);
        Ok(())
    }

    /// Read the stored credential for a host (passphrase or password).
    pub fn credential(&self, id: &str) -> Result<Option<String>> {
        self.secrets.get_secret(id)
    }
}

// ── Known hosts (TOFU) ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KnownHostsFile {
    #[serde(default)]
    hosts: Vec<KnownHost>,
}

fn known_hosts_path() -> PathBuf {
    skillstar_core::infra::paths::ssh_known_hosts_path()
}

/// Load all accepted host-key fingerprints.
pub fn load_known_hosts() -> Vec<KnownHost> {
    let path = known_hosts_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: KnownHostsFile = serde_json::from_str(&content).unwrap_or_default();
    file.hosts
}

/// Record an accepted fingerprint for `host_id`. Replaces any prior entry for
/// the same host id.
pub fn accept_host_key(host_id: &str, host: &str, fingerprint: &str) -> Result<()> {
    let path = known_hosts_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create known_hosts dir")?;
    }
    let mut entries = load_known_hosts();
    entries.retain(|e| e.host_id != host_id);
    entries.push(KnownHost {
        host_id: host_id.to_string(),
        host: host.to_string(),
        fingerprint: fingerprint.to_string(),
    });
    let content = serde_json::to_string_pretty(&KnownHostsFile { hosts: entries })
        .context("serialize known_hosts")?;
    std::fs::write(&path, content).context("write known_hosts")?;
    Ok(())
}

/// Look up the accepted fingerprint for a host id, if any.
pub fn known_fingerprint(host_id: &str) -> Option<String> {
    load_known_hosts()
        .into_iter()
        .find(|e| e.host_id == host_id)
        .map(|e| e.fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AuthMethod;
    use tempfile::TempDir;

    /// RAII guard that points `SKILLSTAR_DATA_DIR` at a temp dir for one test
    /// AND holds the crate-wide env lock, so parallel tests that touch this
    /// env var never interleave (mirrors `skillstar_core::config::test_env_lock`).
    struct DataDirGuard {
        _temp: TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl DataDirGuard {
        fn new() -> Self {
            // Hold the lock for the whole test body — released on Drop.
            let _lock = crate::test_support::env_lock().lock().unwrap();
            let temp = TempDir::new().unwrap();
            // SAFETY: the env lock above serialises all DataDirGuard users,
            // so there is no concurrent mutation of this env var.
            unsafe {
                std::env::set_var("SKILLSTAR_DATA_DIR", temp.path());
            }
            Self { _temp: temp, _lock }
        }
    }

    impl Drop for DataDirGuard {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("SKILLSTAR_DATA_DIR");
            }
        }
    }

    fn sample_host(id: &str) -> SshHostDef {
        SshHostDef {
            id: id.into(),
            display_name: "Prod".into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            auth_method: AuthMethod::Password,
            default_remote_dir: "~/.claude/skills".into(),
        }
    }

    #[test]
    fn load_hosts_empty_when_missing() {
        let _g = DataDirGuard::new();
        assert!(load_hosts().is_empty());
    }

    #[test]
    fn save_and_load_hosts_roundtrip() {
        let _g = DataDirGuard::new();
        let hosts = vec![sample_host("ssh_1"), sample_host("ssh_2")];
        save_hosts(&hosts).unwrap();
        let loaded = load_hosts();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].id, "ssh_1");
    }

    #[test]
    fn hosts_store_add_assigns_id_and_secret() {
        let _g = DataDirGuard::new();
        let store = HostsStore::new(MemSecretStore::new());
        let mut def = sample_host("");
        def.id.clear();
        let created = store.add(def, Some("hunter2")).unwrap();
        assert!(created.id.starts_with("ssh_"));
        assert_eq!(
            store.secrets.get_secret(&created.id).unwrap(),
            Some("hunter2".to_string())
        );
        assert_eq!(load_hosts().len(), 1);
    }

    #[test]
    fn hosts_store_remove_clears_secret() {
        let _g = DataDirGuard::new();
        let store = HostsStore::new(MemSecretStore::new());
        let created = store.add(sample_host("ssh_1"), Some("pw")).unwrap();
        store.remove(&created.id).unwrap();
        assert!(load_hosts().is_empty());
        assert_eq!(store.secrets.get_secret(&created.id).unwrap(), None);
    }

    #[test]
    fn hosts_store_update_moves_secret_on_id_change() {
        let _g = DataDirGuard::new();
        let store = HostsStore::new(MemSecretStore::new());
        store.add(sample_host("old"), Some("secret")).unwrap();
        let mut new_def = sample_host("new");
        new_def.display_name = "Renamed".into();
        store.update("old", new_def, None).unwrap();
        assert_eq!(store.secrets.get_secret("new").unwrap(), Some("secret".into()));
        assert_eq!(store.secrets.get_secret("old").unwrap(), None);
    }

    #[test]
    fn accept_host_key_replaces_prior_entry() {
        let _g = DataDirGuard::new();
        accept_host_key("ssh_1", "host:22", "SHA256:aaa").unwrap();
        accept_host_key("ssh_1", "host:22", "SHA256:bbb").unwrap();
        assert_eq!(known_fingerprint("ssh_1"), Some("SHA256:bbb".into()));
        assert_eq!(load_known_hosts().len(), 1);
    }
}
