//! On-disk fingerprint store.
//!
//! Stored as a single JSON file at `~/.skillstar/config/fingerprints.json`.
//! The store always contains at minimum an immutable [`DeviceFingerprint::original`].

use crate::types::DeviceFingerprint;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors from the fingerprint store layer.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("fingerprint `{0}` not found")]
    NotFound(String),
    #[error("cannot mutate the `original` fingerprint")]
    OriginalImmutable,
    #[error("config root unavailable: {0}")]
    ConfigRoot(String),
}

/// Persistent store of [`DeviceFingerprint`] values.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FingerprintStore {
    /// Currently-applied fingerprint id (used by future projectors).
    pub active_id: Option<String>,
    /// All fingerprints by id, including `"original"`.
    pub items: HashMap<String, DeviceFingerprint>,
}

impl FingerprintStore {
    /// Default file name inside `~/.skillstar/config/`.
    pub const FILE_NAME: &'static str = "fingerprints.json";

    /// Resolve the on-disk path. Honours `SKILLSTAR_HOME` for tests.
    pub fn default_path() -> Result<PathBuf, StoreError> {
        let base = if let Ok(custom) = std::env::var("SKILLSTAR_HOME") {
            PathBuf::from(custom)
        } else {
            dirs_home()?.join(".skillstar")
        };
        Ok(base.join("config").join(Self::FILE_NAME))
    }

    /// Load from `default_path()`, creating a fresh store containing only
    /// `"original"` on first run.
    pub fn load_default() -> Result<Self, StoreError> {
        Self::load_from(&Self::default_path()?)
    }

    /// Load from a specific path; if missing, returns a freshly-seeded store.
    pub fn load_from(path: &Path) -> Result<Self, StoreError> {
        if !path.exists() {
            return Ok(Self::seeded());
        }
        let bytes = std::fs::read(path)?;
        if bytes.is_empty() {
            return Ok(Self::seeded());
        }
        let mut store: Self = serde_json::from_slice(&bytes)?;
        // Ensure "original" always exists even after manual file edits.
        store
            .items
            .entry("original".to_string())
            .or_insert_with(DeviceFingerprint::original);
        Ok(store)
    }

    fn seeded() -> Self {
        let mut items = HashMap::new();
        let original = DeviceFingerprint::original();
        items.insert(original.id.clone(), original);
        Self {
            active_id: Some("original".to_string()),
            items,
        }
    }

    /// Persist to `default_path()` (creates parent dir if needed).
    pub fn save_default(&self) -> Result<(), StoreError> {
        self.save_to(&Self::default_path()?)
    }

    /// Persist to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        // Atomic-ish write: temp then rename.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Look up by id.
    pub fn get(&self, id: &str) -> Result<&DeviceFingerprint, StoreError> {
        self.items
            .get(id)
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    /// All fingerprints, original first, then by created_at desc.
    pub fn list_sorted(&self) -> Vec<&DeviceFingerprint> {
        let mut out: Vec<_> = self.items.values().collect();
        out.sort_by(|a, b| {
            if a.is_original() {
                std::cmp::Ordering::Less
            } else if b.is_original() {
                std::cmp::Ordering::Greater
            } else {
                b.created_at.cmp(&a.created_at)
            }
        });
        out
    }

    /// Insert or update a fingerprint. Refuses `"original"` mutation.
    pub fn upsert(&mut self, fp: DeviceFingerprint) -> Result<(), StoreError> {
        if fp.is_original() && self.items.contains_key("original") {
            return Err(StoreError::OriginalImmutable);
        }
        self.items.insert(fp.id.clone(), fp);
        Ok(())
    }

    /// Remove a fingerprint. Refuses `"original"`.
    pub fn delete(&mut self, id: &str) -> Result<(), StoreError> {
        if id == "original" {
            return Err(StoreError::OriginalImmutable);
        }
        self.items
            .remove(id)
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        if self.active_id.as_deref() == Some(id) {
            self.active_id = Some("original".to_string());
        }
        Ok(())
    }

    /// Set the active fingerprint id (must already be in the store).
    pub fn set_active(&mut self, id: &str) -> Result<(), StoreError> {
        if !self.items.contains_key(id) {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.active_id = Some(id.to_string());
        Ok(())
    }
}

fn dirs_home() -> Result<PathBuf, StoreError> {
    // Avoid the `dirs` crate to keep this crate dep-light; `HOME` is fine on
    // unix and `USERPROFILE` on windows.
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return Ok(PathBuf::from(home));
    }
    if let Ok(profile) = std::env::var("USERPROFILE")
        && !profile.is_empty()
    {
        return Ok(PathBuf::from(profile));
    }
    Err(StoreError::ConfigRoot(
        "neither $HOME nor %USERPROFILE% is set".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn seeded_store_has_original() {
        let s = FingerprintStore::seeded();
        assert!(s.get("original").is_ok());
        assert_eq!(s.active_id.as_deref(), Some("original"));
    }

    #[test]
    fn roundtrip_persists_user_fingerprint() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fp.json");
        let mut s = FingerprintStore::seeded();
        let fp = DeviceFingerprint::generate_chrome();
        let chrome_id = fp.id.clone();
        s.upsert(fp).unwrap();
        s.save_to(&path).unwrap();

        let s2 = FingerprintStore::load_from(&path).unwrap();
        assert!(s2.get(&chrome_id).is_ok());
        assert!(s2.get("original").is_ok());
    }

    #[test]
    fn cannot_delete_original() {
        let mut s = FingerprintStore::seeded();
        assert!(matches!(
            s.delete("original"),
            Err(StoreError::OriginalImmutable)
        ));
    }

    #[test]
    fn deleting_active_falls_back_to_original() {
        let mut s = FingerprintStore::seeded();
        let fp = DeviceFingerprint::generate_chrome();
        let id = fp.id.clone();
        s.upsert(fp).unwrap();
        s.set_active(&id).unwrap();
        s.delete(&id).unwrap();
        assert_eq!(s.active_id.as_deref(), Some("original"));
    }
}
