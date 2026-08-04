//! Lockfile persistence for installed skills.
//!
//! Owns the v5 lockfile schema and mutex. Callers must use this module —
//! `skillstar-core` no longer exports lockfile implementation.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Component, Path};
use std::sync::{Mutex, OnceLock};

static LOCKFILE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_mutex() -> &'static Mutex<()> {
    LOCKFILE_MUTEX.get_or_init(|| Mutex::new(()))
}

/// LockEntry is the skill entry stored in the lockfile.
/// Version 5 records the complete-content hash algorithm version. Unlike the
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash_version: Option<u32>,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_folder: Option<String>,
}

/// Versioned lockfile model for v5.
#[derive(Debug, Serialize, Deserialize)]
pub struct LockfileV5 {
    pub version: u32,
    pub skills: Vec<LockEntry>,
}

impl Default for LockfileV5 {
    fn default() -> Self {
        Self {
            version: 5,
            skills: Vec::new(),
        }
    }
}

impl LockfileV5 {
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
        let mut lockfile: LockfileV5 =
            serde_json::from_str(&content).context("Failed to parse lockfile")?;

        if !(1..=5).contains(&lockfile.version) {
            anyhow::bail!("Unsupported lockfile version: {}", lockfile.version);
        }
        let mut names = BTreeSet::new();
        for entry in &mut lockfile.skills {
            let valid_name = crate::content::validate_skill_name(&entry.name).is_ok();
            let uniqueness_key = if cfg!(windows) {
                entry.name.to_ascii_lowercase()
            } else {
                entry.name.clone()
            };
            if !valid_name || !names.insert(uniqueness_key) {
                anyhow::bail!("Invalid or duplicate Skill lock entry: {:?}", entry.name);
            }
            if entry.source_folder.as_deref() == Some("") {
                entry.source_folder = None;
            }
            if let Some(folder) = entry.source_folder.as_deref()
                && (Path::new(folder).is_absolute()
                    || folder.contains(['\\', '\0'])
                    || !Path::new(folder)
                        .components()
                        .all(|component| matches!(component, Component::Normal(_))))
            {
                anyhow::bail!(
                    "Invalid source folder for Skill '{}': {:?}",
                    entry.name,
                    folder
                );
            }
            if entry.content_hash_version.is_some() && entry.content_hash.is_none() {
                anyhow::bail!("Content hash version without hash for '{}'", entry.name);
            }
            if entry
                .content_hash_version
                .is_some_and(|version| version > crate::content::SNAPSHOT_HASH_VERSION)
            {
                anyhow::bail!(
                    "Unsupported content hash version for '{}': {:?}",
                    entry.name,
                    entry.content_hash_version
                );
            }
        }

        lockfile.version = 5;
        Ok(lockfile)
    }

    /// Save this lockfile as version 5.
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
        if let Some(existing) = self
            .skills
            .iter_mut()
            .find(|skill| skill_names_equal(&skill.name, &entry.name))
        {
            *existing = entry;
        } else {
            self.skills.push(entry);
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.skills.len();
        self.skills
            .retain(|skill| !skill_names_equal(&skill.name, name));
        self.skills.len() < len_before
    }
}

fn skill_names_equal(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Stable call-site name for the current persisted schema.
pub type Lockfile = LockfileV5;

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
            content_hash_version: Some(crate::content::SNAPSHOT_HASH_VERSION),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            source_folder: None,
        }
    }

    #[test]
    fn missing_file_returns_version_5() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");
        let lf = Lockfile::load(&path).unwrap();
        assert_eq!(lf.version, 5);
        assert!(lf.skills.is_empty());
    }

    #[test]
    fn v5_roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        let mut lf = Lockfile::default();
        lf.upsert(make_entry("skill-a"));
        lf.upsert(make_entry("skill-b"));
        lf.save(&path).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.version, 5);
        assert_eq!(loaded.skills.len(), 2);
        assert_eq!(loaded.skills[0].name, "skill-a");
        assert_eq!(loaded.skills[1].name, "skill-b");
    }

    #[test]
    fn v1_style_payload_upgrades_to_v5_without_inventing_a_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        // Simulate a legacy v1 payload with version=1.
        let v1_json = r#"{"version":1,"skills":[{"name":"legacy-skill","git_url":"https://github.com/test/legacy-skill","tree_hash":"old-hash","installed_at":"2025-01-01T00:00:00Z"}]}"#;
        std::fs::write(&path, v1_json).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.version, 5);
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
