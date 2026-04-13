//! Lockfile persistence for installed skills.
//!
//! The lockfile tracks every installed skill's git source, tree hash, and
//! source folder (for monorepo skills). It is the source of truth for
//! what is installed and where it came from.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static LOCKFILE_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_mutex() -> &'static Mutex<()> {
    LOCKFILE_MUTEX.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub git_url: String,
    pub tree_hash: String,
    pub installed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_folder: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Lockfile {
    pub version: u32,
    pub skills: Vec<LockEntry>,
}

impl Lockfile {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                version: 1,
                skills: Vec::new(),
            });
        }
        let content = std::fs::read_to_string(path).context("Failed to read lockfile")?;
        let lockfile: Lockfile =
            serde_json::from_str(&content).context("Failed to parse lockfile")?;
        Ok(lockfile)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, content).context("Failed to write lockfile (temp)")?;
        std::fs::rename(&tmp_path, path).context("Failed to rename lockfile into place")?;
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

/// Get the default lockfile path.
///
/// The caller (main crate) is responsible for providing the correct path
/// via `lockfile_path_override()` or by using `Lockfile::load(path)` directly
/// with a caller-supplied path.
pub fn default_lockfile_path() -> PathBuf {
    // Fallback only — the main crate provides the real path via SKILLSTAR_DATA_DIR
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".skillstar")
        .join("hub")
        .join("lock.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(name: &str) -> LockEntry {
        LockEntry {
            name: name.to_string(),
            git_url: format!("https://github.com/test/{name}"),
            tree_hash: "abc123".to_string(),
            installed_at: "2026-01-01T00:00:00Z".to_string(),
            source_folder: None,
        }
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");
        let lf = Lockfile::load(&path).unwrap();
        assert_eq!(lf.version, 1);
        assert!(lf.skills.is_empty());
    }

    #[test]
    fn round_trip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");

        let mut lf = Lockfile {
            version: 1,
            skills: Vec::new(),
        };
        lf.upsert(make_entry("skill-a"));
        lf.upsert(make_entry("skill-b"));
        lf.save(&path).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.skills.len(), 2);
        assert_eq!(loaded.skills[0].name, "skill-a");
        assert_eq!(loaded.skills[1].name, "skill-b");
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
    fn upsert_with_source_folder() {
        let mut lf = Lockfile::default();
        let mut entry = make_entry("mono-skill");
        entry.source_folder = Some("skills/react".to_string());
        lf.upsert(entry);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.json");
        lf.save(&path).unwrap();

        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(
            loaded.skills[0].source_folder.as_deref(),
            Some("skills/react")
        );
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
