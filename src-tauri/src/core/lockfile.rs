use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub git_url: String,
    pub tree_hash: String,
    pub installed_at: String,
    /// Relative path of the skill folder within a multi-skill repo.
    /// `None` for single-skill repos (the entire repo is the skill).
    /// e.g. `Some("skills/vercel-react")` for a skill inside a monorepo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_folder: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Lockfile {
    pub version: u32,
    pub skills: Vec<LockEntry>,
}

impl Lockfile {
    /// Load lockfile from disk
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

    /// Save lockfile to disk
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content).context("Failed to write lockfile")?;
        Ok(())
    }

    /// Add or update a skill entry
    pub fn upsert(&mut self, entry: LockEntry) {
        if let Some(existing) = self.skills.iter_mut().find(|s| s.name == entry.name) {
            *existing = entry;
        } else {
            self.skills.push(entry);
        }
    }

    /// Remove a skill entry
    pub fn remove(&mut self, name: &str) -> bool {
        let len_before = self.skills.len();
        self.skills.retain(|s| s.name != name);
        self.skills.len() < len_before
    }
}

/// Get the default lockfile path
pub fn lockfile_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agents")
        .join(".skill-lock.json")
}
