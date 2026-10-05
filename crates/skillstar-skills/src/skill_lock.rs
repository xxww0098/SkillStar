//! vercel-labs/skills compatible install lock: `.skill-lock.json` v3.
//!
//! One entry per installed skill identity, keyed by the canonical folder
//! name. The lock is the only install provenance: updates compare the
//! recorded `skill_folder_hash` (a git tree SHA of the skill's folder in the
//! source repo) against the upstream tree and reinstall on mismatch (D-081).
//!
//! Unknown, corrupt, older, or newer versions reset silently to an empty
//! lock — the same tolerance `npx skills` ships.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use skillstar_core::infra::paths;
use skillstar_core::infra::fs_ops;

/// Current lock schema version; mismatches reset the file.
pub const SKILL_LOCK_VERSION: u32 = 3;

/// How the lock classifies a source repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceType {
    Github,
    Git,
    Local,
    Bundle,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Git => "git",
            Self::Local => "local",
            Self::Bundle => "bundle",
        }
    }
}

/// One installed skill, mirroring vercel's per-skill lock fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockEntry {
    /// Normalized short source, e.g. `owner/repo` or `local/<dir>`.
    pub source: String,
    pub source_type: SourceType,
    /// Clone URL used to re-fetch this skill for updates.
    pub source_url: String,
    /// Branch/tag/SHA pinned at install time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Repo-relative folder of the installed skill (empty root skills use `None`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_path: Option<String>,
    /// Git tree SHA of `skill_path` at install time; `None` for local/bundle
    /// sources that never participate in update checks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_folder_hash: Option<String>,
    #[serde(default)]
    pub installed_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// Parsed `.skill-lock.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillLock {
    pub version: u32,
    /// Keyed by canonical skill folder name.
    pub skills: BTreeMap<String, SkillLockEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub last_selected_agents: Vec<String>,
}

impl Default for SkillLock {
    fn default() -> Self {
        Self {
            version: SKILL_LOCK_VERSION,
            skills: BTreeMap::new(),
            last_selected_agents: Vec::new(),
        }
    }
}

pub fn lock_path() -> PathBuf {
    paths::skill_lock_path()
}

/// In-process serialization for read-modify-write cycles. Cross-process
/// writers do not exist: every install goes through this process's facade.
pub fn get_mutex() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

impl SkillLock {
    /// Load the lock; missing, corrupt, or version-mismatched files yield an
    /// empty lock (vercel's silent-reset behavior — never a hard failure).
    pub fn load(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<SkillLock>(&content) {
            Ok(lock) if lock.version == SKILL_LOCK_VERSION => lock,
            _ => Self::default(),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create lock dir '{}'", parent.display()))?;
        }
        // Always persist under the current schema version — a default- or
        // upgraded-constructed lock must round-trip through `load`.
        let mut normalized = self.clone();
        normalized.version = SKILL_LOCK_VERSION;
        let content =
            serde_json::to_string_pretty(&normalized).context("Failed to encode skill lock")?;
        fs_ops::atomic_write(path, content.as_bytes())
            .with_context(|| format!("Failed to write skill lock '{}'", path.display()))
    }

    /// Insert or refresh an entry, preserving the previous `installed_at`.
    pub fn upsert(&mut self, name: &str, entry: SkillLockEntry) {
        let installed_at = self
            .skills
            .get(name)
            .filter(|old| old.installed_at == entry.installed_at || !old.installed_at.is_empty())
            .map(|old| old.installed_at.clone())
            .unwrap_or_else(|| entry.installed_at.clone());
        let mut entry = entry;
        entry.installed_at = installed_at;
        self.skills.insert(name.to_string(), entry);
    }

    pub fn remove(&mut self, name: &str) {
        self.skills.remove(name);
    }

    /// Entries grouped by `(source_url, git_ref)` — the update-check unit:
    /// one upstream tree per group, skills on different refs never compared
    /// against the wrong tree.
    pub fn by_source_group(
        entries: &[(String, SkillLockEntry)],
    ) -> BTreeMap<(String, Option<String>), Vec<(String, SkillLockEntry)>> {
        let mut groups: BTreeMap<(String, Option<String>), Vec<(String, SkillLockEntry)>> =
            BTreeMap::new();
        for (name, entry) in entries {
            groups
                .entry((entry.source_url.clone(), entry.git_ref.clone()))
                .or_default()
                .push((name.clone(), entry.clone()));
        }
        groups
    }
}

/// Read-modify-write helper holding the in-process mutex across the cycle.
pub fn mutate<T>(f: impl FnOnce(&mut SkillLock) -> T) -> Result<T> {
    let _guard = get_mutex()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut lock = SkillLock::load(&lock_path());
    let result = f(&mut lock);
    lock.save(&lock_path())?;
    Ok(result)
}

pub fn load() -> SkillLock {
    SkillLock::load(&lock_path())
}

/// Classification used when writing entries during install.
pub fn classify_source(repo_url: &str) -> SourceType {
    if repo_url.starts_with("file://") {
        SourceType::Local
    } else if repo_url.starts_with("https://github.com/")
        || repo_url.starts_with("git@github.com:")
    {
        SourceType::Github
    } else {
        SourceType::Git
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name_updates: &str) -> SkillLockEntry {
        SkillLockEntry {
            source: "owner/repo".into(),
            source_type: SourceType::Github,
            source_url: "https://github.com/owner/repo.git".into(),
            git_ref: Some("main".into()),
            skill_path: Some(format!("skills/{name_updates}")),
            skill_folder_hash: Some("abc123".into()),
            installed_at: "2026-10-04T10:00:00Z".into(),
            updated_at: "2026-10-04T10:00:00Z".into(),
        }
    }

    #[test]
    fn roundtrip_preserves_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".skill-lock.json");
        let mut lock = SkillLock {
            version: SKILL_LOCK_VERSION,
            ..SkillLock::default()
        };
        lock.upsert("foo", entry("foo"));
        lock.save(&path).unwrap();

        let loaded = SkillLock::load(&path);
        assert_eq!(loaded.skills["foo"].skill_path.as_deref(), Some("skills/foo"));
        assert_eq!(loaded.version, SKILL_LOCK_VERSION);
    }

    #[test]
    fn upsert_preserves_installed_at_and_refreshes_fields() {
        let mut lock = SkillLock::default();
        lock.upsert("foo", entry("foo"));
        let mut refreshed = entry("foo");
        refreshed.installed_at = "2026-10-05T10:00:00Z".into();
        refreshed.skill_folder_hash = Some("def456".into());
        lock.upsert("foo", refreshed);

        assert_eq!(lock.skills["foo"].installed_at, "2026-10-04T10:00:00Z");
        assert_eq!(lock.skills["foo"].skill_folder_hash.as_deref(), Some("def456"));
    }

    #[test]
    fn wrong_or_corrupt_version_resets_silently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".skill-lock.json");

        std::fs::write(&path, "{\"version\":2,\"skills\":{\"a\":{}}}").unwrap();
        assert!(SkillLock::load(&path).skills.is_empty());

        std::fs::write(&path, "not json at all").unwrap();
        assert!(SkillLock::load(&path).skills.is_empty());

        std::fs::write(&path, "{\"version\":99,\"skills\":{}}").unwrap();
        assert!(SkillLock::load(&path).skills.is_empty());
    }

    #[test]
    fn groups_keyed_by_source_and_ref() {
        let mut lock = SkillLock::default();
        lock.upsert("a", entry("a"));
        let mut other_ref = entry("b");
        other_ref.git_ref = Some("dev".into());
        lock.upsert("b", other_ref);

        let groups = SkillLock::by_source_group(&lock.skills.iter().map(|(k,v)| (k.clone(), v.clone())).collect::<Vec<_>>());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[&("https://github.com/owner/repo.git".into(), Some("main".into()))].len(), 1);
    }

    #[test]
    fn classify_matches_url_shapes() {
        assert_eq!(classify_source("https://github.com/o/r.git"), SourceType::Github);
        assert_eq!(classify_source("git@github.com:o/r.git"), SourceType::Github);
        assert_eq!(classify_source("https://gitlab.com/o/r.git"), SourceType::Git);
        assert_eq!(classify_source("file:///tmp/x"), SourceType::Local);
    }
}
