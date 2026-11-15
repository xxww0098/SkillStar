//! SkillStar's own record of each canonical Skill's content as installed.
//!
//! The install lock only carries the upstream git tree SHA, which says nothing
//! about edits made to the canonical copy afterwards. Every staged install
//! records the content hash it just wrote inside `data/skills/library.json`
//! (with read fallback to legacy `install_baselines.json`), so the background
//! auto-updater can refuse to overwrite a Skill whose content no longer matches
//! (D-095, D-113). The `.skill-lock.json` format is untouched.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const BASELINE_VERSION: u32 = 1;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Baselines {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    skills: BTreeMap<String, String>,
}

/// Whether the canonical content still equals its install baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalContent {
    Unchanged,
    Modified,
    /// No baseline (installed before baselines existed, or by another tool)
    /// or the content could not be read; treat as possibly modified.
    Unknown,
}

fn load_legacy() -> Baselines {
    let path = ss_core::infra::paths::skill_install_baselines_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<Baselines>(&content).ok())
        .filter(|baselines| baselines.version == BASELINE_VERSION)
        .unwrap_or_default()
}

fn save_legacy(baselines: &Baselines) {
    let path = ss_core::infra::paths::skill_install_baselines_path();
    let Ok(content) = serde_json::to_string_pretty(baselines) else {
        return;
    };
    let _ = ss_core::infra::fs_ops::atomic_write(&path, content.as_bytes());
}

fn current_hash(name: &str) -> Option<String> {
    crate::content::snapshot(name)
        .ok()
        .map(|snapshot| snapshot.content_hash)
}

/// Record the just-installed content of `names`. Callers hold the Skill
/// transaction lock, which also serializes `library.json`.
pub(crate) fn record(names: &[String]) {
    let updates: Vec<(String, Option<String>)> = names
        .iter()
        .map(|name| (name.clone(), current_hash(name)))
        .collect();
    if let Err(error) = crate::library::set_baseline_hashes(&updates) {
        tracing::warn!(target: "skills", "unable to record Skill install baselines in library.json: {error:#}");
    }
    let legacy_path = ss_core::infra::paths::skill_install_baselines_path();
    if legacy_path.exists() {
        let mut baselines = load_legacy();
        baselines.version = BASELINE_VERSION;
        for (name, hash) in &updates {
            match hash {
                Some(h) => {
                    baselines.skills.insert(name.clone(), h.clone());
                }
                None => {
                    baselines.skills.remove(name);
                }
            }
        }
        save_legacy(&baselines);
    }
}

pub(crate) fn forget(name: &str) {
    let _ = crate::library::set_baseline_hashes(&[(name.to_string(), None)]);
    let legacy_path = ss_core::infra::paths::skill_install_baselines_path();
    if legacy_path.exists() {
        let mut baselines = load_legacy();
        if baselines.skills.remove(name).is_some() {
            save_legacy(&baselines);
        }
    }
}

/// Hash recorded for `name` after the last committed install, if any.
pub(crate) fn recorded_hash(name: &str) -> Option<String> {
    crate::library::baseline_hash(name).or_else(|| load_legacy().skills.get(name).cloned())
}

pub fn local_content(name: &str) -> LocalContent {
    let Some(baseline) = recorded_hash(name) else {
        return LocalContent::Unknown;
    };
    match current_hash(name) {
        Some(current) if current == baseline => LocalContent::Unchanged,
        Some(_) => LocalContent::Modified,
        None => LocalContent::Unknown,
    }
}

/// The upstream-change marker an available update carries when overwriting
/// it could discard local edits; `None` when the content is provably as
/// installed.
pub(crate) fn local_change(name: &str) -> Option<ss_core::types::UpstreamChange> {
    match local_content(name) {
        LocalContent::Unchanged => None,
        LocalContent::Modified => Some(ss_core::types::UpstreamChange::LocalChanges {
            baseline_missing: false,
        }),
        LocalContent::Unknown => Some(ss_core::types::UpstreamChange::LocalChanges {
            baseline_missing: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(name: &str, body: &str) {
        let dir = ss_core::infra::paths::agents_skill_dir(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n{body}\n"),
        )
        .unwrap();
    }

    #[test]
    fn edits_after_install_are_detected_and_forget_drops_the_record() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        write_skill("alpha", "v1");
        assert_eq!(local_content("alpha"), LocalContent::Unknown);

        record(&["alpha".to_string()]);
        assert_eq!(local_content("alpha"), LocalContent::Unchanged);

        write_skill("alpha", "edited");
        assert_eq!(local_content("alpha"), LocalContent::Modified);

        forget("alpha");
        assert_eq!(local_content("alpha"), LocalContent::Unknown);
    }

    #[test]
    fn runtime_artifact_dirs_do_not_mark_skill_modified() {
        let _sandbox = crate::test_sandbox::Sandbox::new();
        write_skill("beta", "v1");
        record(&["beta".to_string()]);
        assert_eq!(local_content("beta"), LocalContent::Unchanged);

        let dir = ss_core::infra::paths::agents_skill_dir("beta");
        for artifact in crate::runtime_ignore::RUNTIME_ARTIFACT_DIRS {
            let sub = dir.join(artifact);
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("generated.txt"), "runtime output").unwrap();
        }
        assert_eq!(local_content("beta"), LocalContent::Unchanged);
    }
}
