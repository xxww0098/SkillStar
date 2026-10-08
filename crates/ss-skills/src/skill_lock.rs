//! Shared-writer install lock: `.skill-lock.json` v3.
//!
//! One entry per installed skill identity, keyed by the canonical folder
//! name. The lock is the only install provenance: updates compare the
//! recorded `skill_folder_hash` (a git tree SHA of the skill's folder in the
//! source repo) against the upstream tree and reinstall on mismatch (D-081).
//! The file layout is the one the `skills` CLI (npx skills) also reads and
//! writes, so installs from either tool stay interoperable.
//!
//! SkillStar never destroys a lock it cannot read: readers see an empty lock
//! for a corrupt or newer-version file, but writers back the file up and fail
//! closed instead of resetting it. Fields and source types SkillStar does not know are carried through
//! a rewrite unchanged. Read-modify-write cycles hold a cross-process file
//! lock and re-read the file under it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ss_core::infra::fs_ops;
use ss_core::infra::paths;

use crate::skill_update::transaction::{LockSlot, ReentrantFileGuard, acquire_reentrant_file_lock};

/// Current lock schema version.
pub const SKILL_LOCK_VERSION: u32 = 3;

/// How the lock classifies a source repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceType {
    Github,
    Git,
    Local,
    Bundle,
    /// A source kind another lock writer recorded that SkillStar cannot
    /// re-fetch; such entries are listed but never updated.
    #[serde(other)]
    Unknown,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Git => "git",
            Self::Local => "local",
            Self::Bundle => "bundle",
            Self::Unknown => "unknown",
        }
    }

    /// Whether updates may re-fetch this entry from its `source_url`.
    pub fn is_updatable(self) -> bool {
        matches!(self, Self::Github | Self::Git)
    }
}

/// Lock entries grouped by their `(source_url, git_ref)` update unit.
pub type SourceGroups = BTreeMap<(String, Option<String>), Vec<(String, SkillLockEntry)>>;

/// One installed skill entry in the shared lock schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLockEntry {
    /// Normalized short source, e.g. `owner/repo` or `local/<dir>`.
    pub source: String,
    pub source_type: SourceType,
    /// Clone URL used to re-fetch this skill for updates.
    pub source_url: String,
    /// Branch/tag/SHA pinned at install time. The shared schema names it
    /// `ref`; early SkillStar builds wrote `gitRef`.
    #[serde(
        rename = "ref",
        alias = "gitRef",
        skip_serializing_if = "Option::is_none"
    )]
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
    /// Fields written by other lock writers, preserved on rewrite.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Parsed `.skill-lock.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillLock {
    pub version: u32,
    /// Keyed by canonical skill folder name.
    pub skills: BTreeMap<String, SkillLockEntry>,
    /// Agents selected by the most recent install, shared with the skills CLI.
    pub last_selected_agents: Vec<String>,
    /// Top-level fields SkillStar does not model (e.g. `dismissed`).
    pub extra: BTreeMap<String, Value>,
    /// Entries as read, written back verbatim while their parsed form is
    /// unchanged, so an unknown `sourceType` string survives a rewrite.
    raw_entries: BTreeMap<String, Value>,
    /// Entries SkillStar could not parse at all; kept, never shown.
    unparsed: BTreeMap<String, Value>,
}

impl Default for SkillLock {
    fn default() -> Self {
        Self {
            version: SKILL_LOCK_VERSION,
            skills: BTreeMap::new(),
            last_selected_agents: Vec::new(),
            extra: BTreeMap::new(),
            raw_entries: BTreeMap::new(),
            unparsed: BTreeMap::new(),
        }
    }
}

/// What is on disk at the lock path.
#[derive(Debug)]
pub enum LockFileState {
    Missing,
    Ready(SkillLock),
    /// Written by an older schema; the shared writer resets these, so
    /// SkillStar does too (after a backup).
    Outdated(u64),
    /// Written by a newer lock writer; never overwritten.
    TooNew(u64),
    Corrupt(String),
}

pub fn lock_path() -> PathBuf {
    paths::skill_lock_path()
}

static LOCK_FILE_MUTEX: Mutex<()> = Mutex::new(());

thread_local! {
    static LOCK_FILE: LockSlot = const { LockSlot::new() };
}

impl<'de> Deserialize<'de> for SkillLock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_json(Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl Serialize for SkillLock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_value()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

/// Cross-process guard for `.skill-lock.json` read-modify-write cycles. The
/// owning thread may re-enter. Load with [`SkillLock::load_for_write`] only
/// after acquiring it.
pub struct SkillLockWriteGuard {
    _inner: ReentrantFileGuard,
}

pub fn lock_for_write() -> Result<SkillLockWriteGuard> {
    let inner = acquire_reentrant_file_lock(
        &LOCK_FILE_MUTEX,
        &LOCK_FILE,
        &paths::skill_lock_write_lock_path(),
        "skill lock file lock",
        None,
    )?;
    Ok(SkillLockWriteGuard { _inner: inner })
}

impl SkillLock {
    pub fn read_state(path: &Path) -> LockFileState {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return LockFileState::Missing;
            }
            Err(error) => return LockFileState::Corrupt(error.to_string()),
        };
        let value: Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(error) => return LockFileState::Corrupt(error.to_string()),
        };
        let lock = match Self::from_json(value) {
            Ok(lock) => lock,
            Err(reason) => return LockFileState::Corrupt(reason.to_string()),
        };
        match lock.version.cmp(&SKILL_LOCK_VERSION) {
            std::cmp::Ordering::Less => LockFileState::Outdated(u64::from(lock.version)),
            std::cmp::Ordering::Greater => LockFileState::TooNew(u64::from(lock.version)),
            std::cmp::Ordering::Equal => LockFileState::Ready(lock),
        }
    }

    /// Structural parse of any version; entries that do not fit the v3 shape
    /// are kept aside rather than failing the whole file.
    fn from_json(value: Value) -> std::result::Result<Self, &'static str> {
        let Value::Object(object) = value else {
            return Err("lock is not a JSON object");
        };
        let version = object
            .get("version")
            .and_then(Value::as_u64)
            .ok_or("lock has no numeric version")?;
        let mut lock = SkillLock {
            version: u32::try_from(version).unwrap_or(u32::MAX),
            ..SkillLock::default()
        };
        for (key, value) in object {
            match key.as_str() {
                "version" => {}
                "skills" => {
                    let Value::Object(skills) = value else {
                        return Err("`skills` is not an object");
                    };
                    for (name, raw) in skills {
                        match serde_json::from_value::<SkillLockEntry>(raw.clone()) {
                            Ok(entry) => {
                                lock.skills.insert(name.clone(), entry);
                                lock.raw_entries.insert(name, raw);
                            }
                            Err(_) => {
                                lock.unparsed.insert(name, raw);
                            }
                        }
                    }
                }
                "lastSelectedAgents" | "last_selected_agents" => {
                    match serde_json::from_value::<Vec<String>>(value.clone()) {
                        Ok(agents) => lock.last_selected_agents = agents,
                        Err(_) => {
                            lock.extra.insert(key, value);
                        }
                    }
                }
                _ => {
                    lock.extra.insert(key, value);
                }
            }
        }
        Ok(lock)
    }

    /// Lenient read for display: anything unreadable yields an empty lock.
    pub fn load(path: &Path) -> Self {
        match Self::read_state(path) {
            LockFileState::Ready(lock) => lock,
            LockFileState::Missing | LockFileState::Outdated(_) => Self::default(),
            LockFileState::TooNew(version) => {
                tracing::warn!(target: "skills", path = %path.display(), version, "skill lock is from a newer writer; showing it as empty");
                Self::default()
            }
            LockFileState::Corrupt(reason) => {
                tracing::warn!(target: "skills", path = %path.display(), reason, "skill lock is unreadable; showing it as empty");
                Self::default()
            }
        }
    }

    /// Read for a rewrite. A newer-version or unreadable file is backed up
    /// and refused, so the rewrite cannot erase entries SkillStar did not
    /// understand. Hold [`lock_for_write`] across load and save.
    pub fn load_for_write(path: &Path) -> Result<Self> {
        match Self::read_state(path) {
            LockFileState::Missing => Ok(Self::default()),
            LockFileState::Ready(lock) => Ok(lock),
            LockFileState::Outdated(version) => {
                let backup = fs_ops::create_rolling_backup(path)?;
                tracing::warn!(target: "skills", path = %path.display(), version, backup = %backup.display(), "resetting outdated skill lock");
                Ok(Self::default())
            }
            LockFileState::TooNew(version) => {
                let backup = fs_ops::create_rolling_backup(path)?;
                bail!(
                    "Skill lock '{}' has version {version}, newer than this SkillStar understands ({SKILL_LOCK_VERSION}); refusing to rewrite it. Nothing was changed. A copy is kept at {}. Update SkillStar, or reset the lock with skill_lock::reset_after_backup().",
                    path.display(),
                    backup.display()
                )
            }
            LockFileState::Corrupt(reason) => {
                let backup = fs_ops::create_rolling_backup(path)?;
                bail!(
                    "Skill lock '{}' is unreadable ({reason}); refusing to rewrite it. Nothing was changed. A copy is kept at {}. Fix the file, or reset the lock with skill_lock::reset_after_backup().",
                    path.display(),
                    backup.display()
                )
            }
        }
    }

    fn to_value(&self) -> Result<Value> {
        let mut skills = Map::new();
        for (name, raw) in &self.unparsed {
            if !self.skills.contains_key(name) {
                skills.insert(name.clone(), raw.clone());
            }
        }
        for (name, entry) in &self.skills {
            let unchanged = self.raw_entries.get(name).filter(|raw| {
                serde_json::from_value::<SkillLockEntry>((*raw).clone())
                    .ok()
                    .as_ref()
                    == Some(entry)
            });
            let value = match unchanged {
                Some(raw) => raw.clone(),
                None => serde_json::to_value(entry).context("Failed to encode skill lock entry")?,
            };
            skills.insert(name.clone(), value);
        }
        let mut root: Map<String, Value> = self.extra.clone().into_iter().collect();
        root.insert("version".into(), Value::from(SKILL_LOCK_VERSION));
        root.insert("skills".into(), Value::Object(skills));
        if !self.last_selected_agents.is_empty() {
            root.insert(
                "lastSelectedAgents".into(),
                Value::from(self.last_selected_agents.clone()),
            );
        }
        Ok(Value::Object(root))
    }

    /// Persist under the current schema version.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create lock dir '{}'", parent.display()))?;
        }
        let content = serde_json::to_string_pretty(&self.to_value()?)
            .context("Failed to encode skill lock")?;
        fs_ops::atomic_write(path, content.as_bytes())
            .with_context(|| format!("Failed to write skill lock '{}'", path.display()))
    }

    /// Every key whose Skill lives in canonical folder `folder`: the folder
    /// name itself first, then keys other writers used (the skills CLI keys
    /// entries by the raw frontmatter name, e.g. `My Skill` for folder
    /// `my-skill`).
    pub fn keys_for_folder(&self, folder: &str) -> Vec<String> {
        let mut keys: Vec<String> = self
            .skills
            .keys()
            .chain(self.unparsed.keys())
            .filter(|key| key.as_str() != folder && folder_for_key(key).as_deref() == Some(folder))
            .cloned()
            .collect();
        keys.sort();
        keys.dedup();
        if self.skills.contains_key(folder) || self.unparsed.contains_key(folder) {
            keys.insert(0, folder.to_string());
        }
        keys
    }

    /// The entry describing canonical folder `folder`, under whichever key it
    /// was recorded.
    pub fn entry_for_folder(&self, folder: &str) -> Option<(&str, &SkillLockEntry)> {
        self.keys_for_folder(folder)
            .into_iter()
            .find_map(|key| self.skills.get_key_value(key.as_str()))
            .map(|(key, entry)| (key.as_str(), entry))
    }

    /// Insert or refresh the entry for canonical folder `name`.
    ///
    /// Entries other writers recorded for the same folder under another key
    /// are folded into this one: fields SkillStar does not model (`extra`,
    /// e.g. `pluginName`) and the first `installed_at` survive, the
    /// new entry's own fields win.
    pub fn upsert(&mut self, name: &str, entry: SkillLockEntry) {
        // Intake records `local/<agent>`. That must not replace a git source
        // another writer stored under the raw frontmatter name.
        if entry.source_type == SourceType::Local
            && self
                .entry_for_folder(name)
                .is_some_and(|(_, existing)| existing.source_type.is_updatable())
        {
            return;
        }
        let mut entry = entry;
        let mut extra = BTreeMap::new();
        let mut installed_at = None;
        for key in self.keys_for_folder(name).into_iter().rev() {
            self.unparsed.remove(&key);
            self.raw_entries.remove(&key);
            if let Some(old) = self.skills.remove(&key) {
                extra.extend(old.extra);
                if !old.installed_at.is_empty() {
                    installed_at = Some(old.installed_at);
                }
            }
        }
        extra.append(&mut entry.extra);
        entry.extra = extra;
        if let Some(installed_at) = installed_at {
            entry.installed_at = installed_at;
        }
        self.skills.insert(name.to_string(), entry);
    }

    /// Drop every entry for canonical folder `name`, whatever its key.
    pub fn remove(&mut self, name: &str) {
        for key in self.keys_for_folder(name) {
            self.skills.remove(&key);
            self.unparsed.remove(&key);
        }
        self.skills.remove(name);
        self.unparsed.remove(name);
    }

    /// Entries grouped by `(source_url, git_ref)` — the update-check unit:
    /// one upstream tree per group, skills on different refs never compared
    /// against the wrong tree.
    pub fn by_source_group(entries: &[(String, SkillLockEntry)]) -> SourceGroups {
        let mut groups: SourceGroups = BTreeMap::new();
        for (name, entry) in entries {
            groups
                .entry((entry.source_url.clone(), entry.git_ref.clone()))
                .or_default()
                .push((name.clone(), entry.clone()));
        }
        groups
    }
}

/// Read-modify-write under the cross-process lock; the file is re-read after
/// the lock is held and the cycle fails closed on an unreadable file.
pub fn mutate<T>(f: impl FnOnce(&mut SkillLock) -> T) -> Result<T> {
    let _guard = lock_for_write()?;
    let path = lock_path();
    let mut lock = SkillLock::load_for_write(&path)?;
    let result = f(&mut lock);
    lock.save(&path)?;
    Ok(result)
}

pub fn load() -> SkillLock {
    SkillLock::load(&lock_path())
}

/// The canonical folder a lock key installs into: other writers derive the
/// folder from the key with the same mapping as
/// [`crate::installer::canonical_skill_name`].
/// `None` for a key no folder can safely carry.
pub fn folder_for_key(key: &str) -> Option<String> {
    crate::materialize::canonical_skill_name(key).ok()
}

/// Fail before any other side effect when the lock could not be rewritten
/// (newer schema or unreadable). Destructive flows call this first so a
/// refused lock write never leaves a Skill half removed.
pub fn ensure_writable() -> Result<()> {
    let _guard = lock_for_write()?;
    SkillLock::load_for_write(&lock_path()).map(|_| ())
}

/// Explicit recovery for a lock writers refuse: keep a copy of the current
/// file, then start over with an empty lock. Installed Skill folders stay;
/// they show without provenance until reinstalled. Returns the backup, or
/// `None` when there was no file.
pub fn reset_after_backup() -> Result<Option<PathBuf>> {
    let _guard = lock_for_write()?;
    let path = lock_path();
    if path.symlink_metadata().is_err() {
        return Ok(None);
    }
    let backup = fs_ops::create_rolling_backup(&path)?;
    SkillLock::default().save(&path)?;
    tracing::warn!(target: "skills", path = %path.display(), backup = %backup.display(), "skill lock reset to empty on request");
    Ok(Some(backup))
}

/// Classification used when writing entries during install.
pub fn classify_source(repo_url: &str) -> SourceType {
    if repo_url.starts_with("file://") {
        SourceType::Local
    } else if repo_url.starts_with("https://github.com/") || repo_url.starts_with("git@github.com:")
    {
        SourceType::Github
    } else {
        SourceType::Git
    }
}

#[cfg(test)]
#[path = "skill_lock_tests.rs"]
mod tests;
