//! Lockfile persistence for installed skills.
//!
//! Owns the v4 lockfile schema and mutex. Callers must use this module —
//! `skillstar-core` no longer exports lockfile implementation.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static LOCKFILE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_mutex() -> &'static Mutex<()> {
    LOCKFILE_MUTEX.get_or_init(|| Mutex::new(()))
}

/// LockEntry is the skill entry stored in the lockfile.
/// Version 4 adds a hash of the complete managed on-disk content. Unlike the
/// Git tree hash, this changes when the user edits or adds a local file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub git_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    pub tree_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_folder: Option<String>,
}

/// Versioned lockfile model for v4.
#[derive(Debug, Serialize, Deserialize)]
pub struct LockfileV3 {
    pub version: u32,
    pub skills: Vec<LockEntry>,
}

impl Default for LockfileV3 {
    fn default() -> Self {
        Self {
            version: 4,
            skills: Vec::new(),
        }
    }
}

impl LockfileV3 {
    /// Load a lockfile from disk, upgrading any older version in memory.
    ///
    /// Legacy entries have no content baseline and therefore fail closed on
    /// their first update instead of silently assuming a modified tree is clean.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path).context("Failed to read lockfile")?;
        // Older payloads remain field-compatible because newly-added fields
        // default during deserialization.
        let mut lockfile: LockfileV3 =
            serde_json::from_str(&content).context("Failed to parse lockfile")?;

        lockfile.version = 4;
        Ok(lockfile)
    }

    /// Save this lockfile as version 4.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        skillstar_core::infra::fs_ops::atomic_write(path, content.as_bytes())
            .context("Failed to write lockfile atomically")?;
        Ok(())
    }

    pub fn upsert(&mut self, entry: LockEntry) {
        if let Some(existing) = self.skills.iter_mut().find(|s| s.name == entry.name) {
            *existing = entry;
        } else {
            self.skills.push(entry);
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.skills.len();
        self.skills.retain(|s| s.name != name);
        self.skills.len() < len_before
    }
}

/// Alias Lockfile to LockfileV3 for backward compatibility with existing call sites.
pub type Lockfile = LockfileV3;

/// App-specific lockfile path under the SkillStar data root.
pub fn lockfile_path() -> std::path::PathBuf {
    skillstar_core::infra::paths::lockfile_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> LockEntry {
        LockEntry {
            name: name.to_string(),
            git_url: format!("https://github.com/test/{name}"),
            git_ref: None,
            tree_hash: "abc123".to_string(),
            content_hash: Some("managed-content".to_string()),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            source_folder: None,
        }
    }

    #[test]
    fn missing_file_returns_version_4() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");
        let lf = Lockfile::load(&path).unwrap();
        assert_eq!(lf.version, 4);
        assert!(lf.skills.is_empty());
    }

    #[test]
    fn v4_roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        let mut lf = Lockfile::default();
        lf.upsert(make_entry("skill-a"));
        lf.upsert(make_entry("skill-b"));
        lf.save(&path).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.version, 4);
        assert_eq!(loaded.skills.len(), 2);
        assert_eq!(loaded.skills[0].name, "skill-a");
        assert_eq!(loaded.skills[1].name, "skill-b");
    }

    #[test]
    fn v1_style_payload_upgrades_to_v4_without_inventing_a_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        // Simulate a legacy v1 payload with version=1.
        let v1_json = r#"{"version":1,"skills":[{"name":"legacy-skill","git_url":"https://github.com/test/legacy-skill","tree_hash":"old-hash","installed_at":"2025-01-01T00:00:00Z"}]}"#;
        std::fs::write(&path, v1_json).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.version, 4);
        assert_eq!(loaded.skills.len(), 1);
        assert_eq!(loaded.skills[0].name, "legacy-skill");
        assert_eq!(loaded.skills[0].tree_hash, "old-hash");
        assert_eq!(loaded.skills[0].content_hash, None);
    }

    #[test]
    fn source_folder_and_tree_hash_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        let mut lf = Lockfile::default();
        let mut entry = make_entry("mono-skill");
        entry.git_ref = Some("release/v2".to_string());
        entry.source_folder = Some("skills/react".to_string());
        lf.upsert(entry);
        lf.save(&path).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(
            loaded.skills[0].source_folder.as_deref(),
            Some("skills/react")
        );
        assert_eq!(loaded.skills[0].tree_hash, "abc123");
        assert_eq!(loaded.skills[0].git_ref.as_deref(), Some("release/v2"));
    }

    #[test]
    fn upsert_updates_existing_entry() {
        let mut lf = Lockfile::default();
        lf.upsert(make_entry("skill-a"));

        let mut updated = make_entry("skill-a");
        updated.tree_hash = "new-hash".to_string();
        lf.upsert(updated);

        assert_eq!(lf.skills.len(), 1);
        assert_eq!(lf.skills[0].tree_hash, "new-hash");
    }

    #[test]
    fn remove_returns_true_when_found() {
        let mut lf = Lockfile::default();
        lf.upsert(make_entry("skill-a"));
        lf.upsert(make_entry("skill-b"));

        assert!(lf.remove("skill-a"));
        assert_eq!(lf.skills.len(), 1);
        assert_eq!(lf.skills[0].name, "skill-b");
    }

    #[test]
    fn remove_returns_false_when_not_found() {
        let mut lf = Lockfile::default();
        lf.upsert(make_entry("skill-a"));
        assert!(!lf.remove("nonexistent"));
        assert_eq!(lf.skills.len(), 1);
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deep").join("lock.json");

        let lf = Lockfile::default();
        lf.save(&path).unwrap();

        assert!(path.exists());
    }
}
