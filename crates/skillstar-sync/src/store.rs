//! Persistence for S3 sync targets and their credentials.
//!
//! Two storage tiers, mirroring `skillstar_ssh::store`:
//!
//! - **Target metadata** (`~/.skillstar/config/s3_targets.toml`) — a
//!   `Vec<S3TargetDef>` containing only non-sensitive fields (endpoint, region,
//!   bucket, prefix, access key id). Safe to back up.
//! - **Credentials** — `secret_access_key` goes through the system keyring via
//!   the [`SecretStore`] trait. The production impl uses `keyring` v4; tests use
//!   [`MemSecretStore`].

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::types::S3TargetDef;

/// Service name used for every keyring entry. The account name is the target `id`.
pub const KEYRING_SERVICE: &str = "skillstar-sync";

// ── Target metadata persistence ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TargetsFile {
    #[serde(default)]
    targets: Vec<S3TargetDef>,
}

fn targets_config_path() -> PathBuf {
    skillstar_core::infra::paths::s3_targets_config_path()
}

/// Load all S3 target definitions from disk. Missing/corrupt file → empty list.
pub fn load_targets() -> Vec<S3TargetDef> {
    let path = targets_config_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let file: TargetsFile = toml::from_str(&content).unwrap_or_default();
    // Normalise prefix on read so downstream key math is consistent.
    file.targets
        .into_iter()
        .map(|mut t| {
            t.prefix = t.normalised_prefix();
            t
        })
        .collect()
}

/// Persist the full target list to disk (atomic write via tmp + rename).
pub fn save_targets(targets: &[S3TargetDef]) -> Result<()> {
    let path = targets_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("create s3_targets config dir")?;
    }
    let file = TargetsFile {
        targets: targets.to_vec(),
    };
    let content = toml::to_string_pretty(&file).context("serialize s3_targets.toml")?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, content).context("write s3_targets.toml.tmp")?;
    std::fs::rename(&tmp, &path).context("rename s3_targets.toml")?;
    Ok(())
}

// ── Secret store abstraction ────────────────────────────────────────

pub trait SecretStore: Send + Sync {
    fn get_secret(&self, target_id: &str) -> Result<Option<String>>;
    fn set_secret(&self, target_id: &str, value: &str) -> Result<()>;
    fn delete_secret(&self, target_id: &str) -> Result<()>;
}

/// Production secret store backed by the OS keyring.
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get_secret(&self, target_id: &str) -> Result<Option<String>> {
        match keyring::Entry::new(KEYRING_SERVICE, target_id) {
            Ok(entry) => match entry.get_password() {
                Ok(pw) => Ok(Some(pw)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(anyhow::anyhow!(err).context("keyring get_password")),
            },
            Err(err) => Err(anyhow::anyhow!(err).context("keyring entry new")),
        }
    }

    fn set_secret(&self, target_id: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, target_id)
            .context("keyring entry new")?;
        entry.set_password(value).context("keyring set_password")?;
        Ok(())
    }

    fn delete_secret(&self, target_id: &str) -> Result<()> {
        match keyring::Entry::new(KEYRING_SERVICE, target_id) {
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
    map: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl SecretStore for MemSecretStore {
    fn get_secret(&self, target_id: &str) -> Result<Option<String>> {
        Ok(self.map.lock().unwrap().get(target_id).cloned())
    }
    fn set_secret(&self, target_id: &str, value: &str) -> Result<()> {
        self.map
            .lock()
            .unwrap()
            .insert(target_id.to_string(), value.to_string());
        Ok(())
    }
    fn delete_secret(&self, target_id: &str) -> Result<()> {
        self.map.lock().unwrap().remove(target_id);
        Ok(())
    }
}

// ── High-level target CRUD ──────────────────────────────────────────

/// Convenience façade combining target metadata + credential storage.
///
/// Constructed with a [`SecretStore`] impl; production code uses
/// [`KeyringSecretStore`], tests pass a [`MemSecretStore`].
pub struct TargetsStore<S: SecretStore> {
    secrets: S,
}

impl<S: SecretStore> TargetsStore<S> {
    pub fn new(secrets: S) -> Self {
        Self { secrets }
    }

    /// Generate a target id unique against the current on-disk list.
    pub fn fresh_id() -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("s3_{now_ms}")
    }

    /// Insert a new target. If `def.id` is empty it is auto-generated and the
    /// populated def is returned. `secret_access_key` is written to the secret
    /// store when non-empty.
    pub fn add(&self, mut def: S3TargetDef, secret: Option<&str>) -> Result<S3TargetDef> {
        if def.id.trim().is_empty() {
            def.id = Self::fresh_id();
        }
        def.prefix = def.normalised_prefix();
        if let Some(sk) = secret.filter(|s| !s.is_empty()) {
            self.secrets.set_secret(&def.id, sk)?;
        }
        let mut targets = load_targets();
        if targets.iter().any(|t| t.id == def.id) {
            anyhow::bail!("S3 target id '{}' already exists", def.id);
        }
        targets.push(def.clone());
        save_targets(&targets)?;
        Ok(def)
    }

    /// Replace a target by id. If `secret` is `Some`, the stored secret is
    /// updated (empty string clears it); `None` leaves the existing secret.
    pub fn update(&self, id: &str, mut def: S3TargetDef, secret: Option<&str>) -> Result<()> {
        def.prefix = def.normalised_prefix();
        let mut targets = load_targets();
        let target = targets
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| anyhow::anyhow!("S3 target '{}' not found", id))?;
        // If the id changed, move the secret so credentials follow the target.
        if def.id != *id
            && let Some(sk) = self.secrets.get_secret(id)? {
                self.secrets.set_secret(&def.id, &sk)?;
                self.secrets.delete_secret(id)?;
            }
        if let Some(sk) = secret {
            if sk.is_empty() {
                self.secrets.delete_secret(&def.id)?;
            } else {
                self.secrets.set_secret(&def.id, sk)?;
            }
        }
        *target = def;
        save_targets(&targets)
    }

    /// Remove a target by id and delete its secret.
    pub fn remove(&self, id: &str) -> Result<()> {
        let mut targets = load_targets();
        let before = targets.len();
        targets.retain(|t| t.id != id);
        if targets.len() == before {
            anyhow::bail!("S3 target '{}' not found", id);
        }
        save_targets(&targets)?;
        // Best-effort secret cleanup.
        let _ = self.secrets.delete_secret(id);
        Ok(())
    }

    /// Read the stored secret access key for a target.
    pub fn secret(&self, id: &str) -> Result<Option<String>> {
        self.secrets.get_secret(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::env_lock;

    fn sample(id: &str) -> S3TargetDef {
        S3TargetDef {
            id: id.to_string(),
            display_name: format!("t-{id}"),
            endpoint_url: "https://example.r2.cloudflarestorage.com".to_string(),
            region: "auto".to_string(),
            bucket: "skillstar".to_string(),
            prefix: "skillstar/".to_string(),
            access_key_id: "AKIA".to_string(),
            force_path_style: false,
        }
    }

    #[test]
    fn crud_round_trip() {
        let _g = env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: test-scoped; held under env_lock().
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }

        let store = TargetsStore::new(MemSecretStore::default());
        let added = store.add(sample(""), Some("shh")).unwrap();
        assert!(!added.id.is_empty());
        assert_eq!(store.secret(&added.id).unwrap(), Some("shh".to_string()));

        let list = load_targets();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, added.id);

        // update moves secret when id changes
        let mut moved = sample(&added.id);
        moved.id = "s3_moved".to_string();
        store.update(&added.id, moved, None).unwrap();
        assert_eq!(store.secret("s3_moved").unwrap(), Some("shh".to_string()));
        assert_eq!(store.secret(&added.id).unwrap(), None);

        // update can clear secret
        store.update("s3_moved", sample("s3_moved"), Some("")).unwrap();
        assert_eq!(store.secret("s3_moved").unwrap(), None);

        store.remove("s3_moved").unwrap();
        assert!(load_targets().is_empty());
    }

    #[test]
    fn prefix_normalised_on_save() {
        let _g = env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }

        let store = TargetsStore::new(MemSecretStore::default());
        let mut t = sample("");
        t.prefix = "no/slash".to_string();
        let added = store.add(t, None).unwrap();
        assert_eq!(added.prefix, "no/slash/");
    }
}
