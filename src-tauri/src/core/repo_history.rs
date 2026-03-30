use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single entry in the repo scan history.
///
/// Persisted in `~/.skillstar/repo_history.json`.
/// Tracks every repo the user has scanned, even if no skills were installed,
/// so the UI can offer a quick-reuse dropdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoHistoryEntry {
    /// Short identifier, e.g. "vercel-labs/skills"
    pub source: String,
    /// Full clone URL, e.g. "https://github.com/vercel-labs/skills.git"
    pub source_url: String,
    /// ISO 8601 timestamp of the last scan
    pub last_used: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RepoHistoryFile {
    entries: Vec<RepoHistoryEntry>,
}

fn history_path() -> PathBuf {
    super::paths::data_root().join("repo_history.json")
}

/// Load all repo history entries, sorted by most-recently-used first.
pub fn list_entries() -> Vec<RepoHistoryEntry> {
    let path = history_path();
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut file: RepoHistoryFile = serde_json::from_str(&content).unwrap_or_default();
    // Sort by last_used descending (most recent first)
    file.entries.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    file.entries
}

/// Add or update a repo history entry.
///
/// If the source already exists, updates its timestamp. Otherwise appends a new entry.
/// Caps the history at 30 entries to keep the dropdown manageable.
pub fn upsert_entry(source: &str, source_url: &str) -> Result<()> {
    let path = history_path();
    let mut file = if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str::<RepoHistoryFile>(&content).unwrap_or_default()
    } else {
        RepoHistoryFile::default()
    };

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(existing) = file.entries.iter_mut().find(|e| e.source == source) {
        existing.last_used = now;
        existing.source_url = source_url.to_string();
    } else {
        file.entries.push(RepoHistoryEntry {
            source: source.to_string(),
            source_url: source_url.to_string(),
            last_used: now,
        });
    }

    // Cap at 30 entries, evicting oldest
    file.entries.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    file.entries.truncate(30);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create repo history directory")?;
    }

    let content =
        serde_json::to_string_pretty(&file).context("Failed to serialize repo history")?;
    std::fs::write(&path, content).context("Failed to write repo history")?;

    Ok(())
}

/// Clear all repo history entries.
///
/// Returns the number of entries that were removed.
pub fn clear_history() -> Result<usize> {
    let path = history_path();
    if !path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let file: RepoHistoryFile = serde_json::from_str(&content).unwrap_or_default();
    let count = file.entries.len();
    if count > 0 {
        std::fs::remove_file(&path).context("Failed to remove repo history file")?;
    }
    Ok(count)
}

/// Return the number of entries in repo history.
pub fn entry_count() -> usize {
    let path = history_path();
    if !path.exists() {
        return 0;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let file: RepoHistoryFile = serde_json::from_str(&content).unwrap_or_default();
    file.entries.len()
}
