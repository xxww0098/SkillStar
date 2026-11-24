//! Central skill management ledger (`data/skills/library.json`, D-113).
//!
//! `.skill-lock.json` stays 100% compatible with the upstream `skills` CLI
//! schema, while `library.json` holds SkillStar's richer management state in one
//! locked atomic ledger:
//! - origin classification (`github` / `git` / `local` / `channel` / ...)
//! - coexistence display alias (`called`) when `<name>-<owner>` was allocated
//! - install baseline hash (`baseline_hash`, migrated from legacy
//!   `install_baselines.json`)
//! - desired agents (`desired_agents`) for ref-counted `~/.agents/skills` and
//!   anti-double-loading coordination
//! - per-agent applied deployment records (`mode`, `content_hash`, mtime fast
//!   path)

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use ss_core::infra::{fs_ops, paths};

use crate::local_identity;
use crate::skill_lock::{self, SkillLock, SkillLockEntry, SourceType};

pub const LIBRARY_VERSION: u32 = 1;

fn default_version() -> u32 {
    LIBRARY_VERSION
}

/// Authoritative provenance/ownership category for a managed skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillOrigin {
    Github,
    Git,
    Local,
    Bundle,
    WellKnown,
    Channel,
    #[default]
    #[serde(other)]
    Unknown,
}

impl From<SourceType> for SkillOrigin {
    fn from(value: SourceType) -> Self {
        match value {
            SourceType::Github => Self::Github,
            SourceType::Git => Self::Git,
            SourceType::Local => Self::Local,
            SourceType::Bundle => Self::Bundle,
            SourceType::WellKnown => Self::WellKnown,
            SourceType::Unknown => Self::Unknown,
        }
    }
}

/// Recorded deployment state for one `(skill, target)` pair.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedTargetRecord {
    /// `"symlink"`, `"copy"`, or `"shared"`.
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree_max_mtime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    #[serde(default)]
    pub updated_at: String,
}

/// One canonical skill's record in `data/skills/library.json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySkillEntry {
    #[serde(default)]
    pub origin: SkillOrigin,
    /// Original skill name from upstream frontmatter before coexistence
    /// disambiguation (`<name>-<owner>`), when renamed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub called: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_hash: Option<String>,
    #[serde(default)]
    pub installed_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub desired_agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub applied: BTreeMap<String, AppliedTargetRecord>,
}

/// Parsed `data/skills/library.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillLibrary {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub skills: BTreeMap<String, LibrarySkillEntry>,
}

impl Default for SkillLibrary {
    fn default() -> Self {
        Self {
            version: LIBRARY_VERSION,
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct LegacyBaselines {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    skills: BTreeMap<String, String>,
}

fn read_legacy_baselines() -> BTreeMap<String, String> {
    let path = paths::skill_install_baselines_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<LegacyBaselines>(&content).ok())
        .filter(|b| b.version == 1)
        .map(|b| b.skills)
        .unwrap_or_default()
}

fn entry_from_lock(name: &str, lock_entry: &SkillLockEntry, baselines: &BTreeMap<String, String>) -> LibrarySkillEntry {
    let called = lock_entry
        .extra
        .get("called")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    LibrarySkillEntry {
        origin: SkillOrigin::from(lock_entry.source_type),
        called,
        source: (!lock_entry.source.is_empty()).then(|| lock_entry.source.clone()),
        source_url: (!lock_entry.source_url.is_empty()).then(|| lock_entry.source_url.clone()),
        git_ref: lock_entry.git_ref.clone(),
        subpath: lock_entry.skill_path.clone(),
        folder_hash: lock_entry.skill_folder_hash.clone(),
        baseline_hash: baselines.get(name).cloned(),
        installed_at: lock_entry.installed_at.clone(),
        updated_at: lock_entry.updated_at.clone(),
        desired_agents: Vec::new(),
        applied: BTreeMap::new(),
    }
}

fn seed_from_legacy() -> SkillLibrary {
    let mut library = SkillLibrary::default();
    let lock = skill_lock::load();
    let baselines = read_legacy_baselines();
    for (name, lock_entry) in &lock.skills {
        library
            .skills
            .insert(name.clone(), entry_from_lock(name, lock_entry, &baselines));
    }
    let installed = paths::agents_skills_root();
    if let Ok(entries) = std::fs::read_dir(&installed) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };
            if name.starts_with('.') || library.skills.contains_key(&name) {
                continue;
            }
            let origin = if matches!(local_identity::read_local_identity(&path), Ok(Some(_))) {
                SkillOrigin::Local
            } else {
                SkillOrigin::Unknown
            };
            let now = chrono::Utc::now().to_rfc3339();
            library.skills.insert(
                name.clone(),
                LibrarySkillEntry {
                    origin,
                    baseline_hash: baselines.get(&name).cloned(),
                    installed_at: now.clone(),
                    updated_at: now,
                    ..Default::default()
                },
            );
        }
    }
    library
}

fn read_disk_only(path: &Path) -> Option<SkillLibrary> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<SkillLibrary>(&content).ok()
}

/// Lenient read for display & queries. Seeds an in-memory view from
/// `.skill-lock.json` and `install_baselines.json` if `library.json` has not
/// been written yet.
pub fn load() -> SkillLibrary {
    let path = paths::skills_library_path();
    if let Some(lib) = read_disk_only(&path) {
        return lib;
    }
    seed_from_legacy()
}

/// Atomically mutate `data/skills/library.json` under the shared skills write lock.
pub fn mutate<T>(f: impl FnOnce(&mut SkillLibrary) -> T) -> Result<T> {
    let _guard = skill_lock::lock_for_write()?;
    let path = paths::skills_library_path();
    let mut library = match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<SkillLibrary>(&content) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(
                    target: "skills",
                    path = %path.display(),
                    %error,
                    "corrupt library.json; rebuilding from lock and disk"
                );
                let _ = fs_ops::create_rolling_backup(&path);
                seed_from_legacy()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => seed_from_legacy(),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()));
        }
    };
    library.version = LIBRARY_VERSION;
    let out = f(&mut library);
    let json = serde_json::to_string_pretty(&library).context("Failed to serialize library.json")?;
    fs_ops::atomic_write(&path, json.as_bytes())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(out)
}

/// Return `true` when `library.json` explicitly records `name` with `Local` origin.
pub fn is_local_origin(name: &str) -> bool {
    let path = paths::skills_library_path();
    read_disk_only(&path)
        .and_then(|lib| lib.skills.get(name).map(|e| e.origin == SkillOrigin::Local))
        .unwrap_or(false)
}

/// Record or update a remote/bundle/well-known install in `library.json`.
#[allow(clippy::too_many_arguments)]
pub fn record_remote_install(
    name: &str,
    lock_entry: &SkillLockEntry,
    called: Option<String>,
    baseline_hash: Option<String>,
    is_channel: bool,
) -> Result<()> {
    let origin = if is_channel {
        SkillOrigin::Channel
    } else {
        SkillOrigin::from(lock_entry.source_type)
    };
    mutate(|lib| {
        let entry = lib.skills.entry(name.to_string()).or_default();
        if entry.installed_at.is_empty() {
            entry.installed_at = lock_entry.installed_at.clone();
        }
        entry.origin = origin;
        entry.called = called;
        entry.source = (!lock_entry.source.is_empty()).then(|| lock_entry.source.clone());
        entry.source_url = (!lock_entry.source_url.is_empty()).then(|| lock_entry.source_url.clone());
        entry.git_ref = lock_entry.git_ref.clone();
        entry.subpath = lock_entry.skill_path.clone();
        entry.folder_hash = lock_entry.skill_folder_hash.clone();
        if let Some(hash) = baseline_hash {
            entry.baseline_hash = Some(hash);
        }
        entry.updated_at = lock_entry.updated_at.clone();
    })
}

/// Record or refresh a locally authored skill in `library.json`.
pub fn record_local_skill(name: &str) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    mutate(|lib| {
        let entry = lib.skills.entry(name.to_string()).or_default();
        if entry.installed_at.is_empty() {
            entry.installed_at = now.clone();
        }
        entry.origin = SkillOrigin::Local;
        entry.called = None;
        entry.source = None;
        entry.source_url = None;
        entry.git_ref = None;
        entry.subpath = None;
        entry.folder_hash = None;
        entry.updated_at = now;
    })
}

/// Remove a canonical skill from `library.json`.
pub fn forget_skill(name: &str) -> Result<()> {
    mutate(|lib| {
        lib.skills.remove(name);
    })
}

/// Original frontmatter name when disambiguated (`<name>-<owner>`), if any.
pub fn called_name(name: &str) -> Option<String> {
    load().skills.get(name).and_then(|e| e.called.clone())
}

/// Return recorded baseline hash in `library.json`, if present.
pub fn baseline_hash(name: &str) -> Option<String> {
    load().skills.get(name).and_then(|e| e.baseline_hash.clone())
}

/// Batch-update `baseline_hash` entries in `library.json`.
pub fn set_baseline_hashes(updates: &[(String, Option<String>)]) -> Result<()> {
    mutate(|lib| {
        let now = chrono::Utc::now().to_rfc3339();
        for (name, hash) in updates {
            match hash {
                Some(h) => {
                    let entry = lib.skills.entry(name.clone()).or_default();
                    if entry.installed_at.is_empty() {
                        entry.installed_at = now.clone();
                    }
                    entry.baseline_hash = Some(h.clone());
                }
                None => {
                    if let Some(entry) = lib.skills.get_mut(name) {
                        entry.baseline_hash = None;
                    }
                }
            }
        }
    })
}

/// Reconcile `library.json` against canonical skills on disk and `.skill-lock.json`.
pub fn reconcile_library_from_disk() -> Result<()> {
    let lock = SkillLock::load(&paths::skill_lock_path());
    let baselines = read_legacy_baselines();
    let installed_root = paths::agents_skills_root();
    let mut on_disk = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(&installed_root) {
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(ToString::to_string) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() || fs_ops::is_link(&path) {
                on_disk.insert(name);
            }
        }
    }
    mutate(|lib| {
        let now = chrono::Utc::now().to_rfc3339();
        lib.skills.retain(|name, _| on_disk.contains(name));
        for name in &on_disk {
            let skill_dir = installed_root.join(name);
            if let Some(lock_entry) = lock.skills.get(name) {
                let entry = lib
                    .skills
                    .entry(name.clone())
                    .or_insert_with(|| entry_from_lock(name, lock_entry, &baselines));
                if entry.origin == SkillOrigin::Unknown {
                    entry.origin = SkillOrigin::from(lock_entry.source_type);
                }
                if entry.baseline_hash.is_none() {
                    entry.baseline_hash = baselines.get(name).cloned();
                }
            } else if !lib.skills.contains_key(name) {
                let origin = if matches!(local_identity::read_local_identity(&skill_dir), Ok(Some(_))) {
                    SkillOrigin::Local
                } else {
                    SkillOrigin::Unknown
                };
                lib.skills.insert(
                    name.clone(),
                    LibrarySkillEntry {
                        origin,
                        baseline_hash: baselines.get(name).cloned(),
                        installed_at: now.clone(),
                        updated_at: now.clone(),
                        ..Default::default()
                    },
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_seeds_from_lock_and_legacy_baselines_and_roundtrips() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        let canonical = paths::agents_skill_dir("alpha");
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::write(canonical.join("SKILL.md"), "---\nname: alpha\ndescription: d\n---\n").unwrap();

        let now = "2026-01-01T00:00:00Z".to_string();
        let lock_entry = SkillLockEntry {
            source: "acme/skills".to_string(),
            source_type: SourceType::Github,
            source_url: "https://github.com/acme/skills.git".to_string(),
            git_ref: None,
            skill_path: Some("skills/alpha".to_string()),
            skill_folder_hash: Some("deadbeef".to_string()),
            installed_at: now.clone(),
            updated_at: now,
            extra: BTreeMap::from([("called".to_string(), serde_json::json!("orig-alpha"))]),
        };
        skill_lock::mutate(|lock| {
            lock.upsert("alpha", lock_entry.clone());
        })
        .unwrap();

        let legacy_path = paths::skill_install_baselines_path();
        std::fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
        std::fs::write(
            &legacy_path,
            r#"{"version":1,"skills":{"alpha":"sha256:legacy"}}"#,
        )
        .unwrap();

        let loaded = load();
        let entry = loaded.skills.get("alpha").unwrap();
        assert_eq!(entry.origin, SkillOrigin::Github);
        assert_eq!(entry.baseline_hash.as_deref(), Some("sha256:legacy"));
        assert_eq!(entry.called.as_deref(), Some("orig-alpha"));

        mutate(|lib| {
            let entry = lib.skills.entry("alpha".to_string()).or_default();
            entry.desired_agents.push("claude-code".to_string());
            entry.applied.insert(
                "claude-code".to_string(),
                AppliedTargetRecord {
                    mode: "copy".to_string(),
                    content_hash: Some("h1".to_string()),
                    tree_max_mtime_ms: Some(12345),
                    file_count: Some(1),
                    updated_at: "2026-01-02T00:00:00Z".to_string(),
                },
            );
        })
        .unwrap();

        let persisted = load();
        assert_eq!(persisted.skills["alpha"].desired_agents, vec!["claude-code"]);
        assert_eq!(persisted.skills["alpha"].applied["claude-code"].mode, "copy");
    }
}
