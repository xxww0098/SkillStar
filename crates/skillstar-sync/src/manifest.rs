//! Cloud manifest construction and parsing.
//!
//! On push, a [`Manifest`] is built from the locally installed skills:
//! - git-backed hub skills → [`ManifestEntry::Hub`] (metadata only)
//! - local-authored skills → [`ManifestEntry::Local`] (after tarball upload)
//!
//! On pull, the manifest is downloaded and each entry is annotated with whether
//! it is already installed on this device.

use std::collections::HashMap;

use crate::types::{Manifest, ManifestEntry, ManifestEntryView, PushSummary};
use skillstar_core::types::Skill;

/// A local skill's tarball metadata, produced by [`crate::local_pack::pack_skill`]
/// and threaded into the manifest builder.
pub struct PackedLocal {
    pub sha256: String,
    pub size_bytes: u64,
    pub tarball_key: String,
    pub uploaded_at: String,
}

/// Partition `Vec<Skill>` into the (hub, local) halves the manifest cares about.
/// Local detection reuses `skillstar_skills::local_skill::is_local_skill` so we
/// agree with the rest of the codebase even if `skill_type` is stale.
pub fn partition_skills(skills: Vec<Skill>) -> (Vec<Skill>, Vec<Skill>) {
    let mut hub = Vec::new();
    let mut local = Vec::new();
    for s in skills {
        if skillstar_skills::local_skill::is_local_skill(&s.name) {
            local.push(s);
        } else {
            hub.push(s);
        }
    }
    (hub, local)
}

/// Build the full manifest, threading per-local-skill tarball metadata in.
///
/// `local_meta` maps skill name → packed tarball info (filled by the sync
/// layer after upload). Hub skills with an empty `git_url` are dropped (no
/// restorable source). Local skills without an entry are dropped (upload failed
/// or skipped intentionally). Per-hub-skill `source_folder` is enriched from the
/// on-disk lockfile so monorepo skills restore to the right sub-folder.
pub fn build_manifest(
    hub: Vec<Skill>,
    local: Vec<Skill>,
    local_meta: HashMap<String, PackedLocal>,
    device_id: String,
    generated_at: String,
) -> Manifest {
    let source_folders = load_source_folders();

    let mut entries: Vec<ManifestEntry> = Vec::with_capacity(hub.len() + local.len());

    for s in hub {
        if s.git_url.trim().is_empty() {
            continue;
        }
        entries.push(ManifestEntry::Hub {
            name: s.name.clone(),
            git_url: s.git_url,
            source_folder: source_folders.get(&s.name).cloned(),
            tree_hash: s.tree_hash,
            description: s.description,
        });
    }
    for s in local {
        let Some(meta) = local_meta.get(&s.name) else {
            continue;
        };
        entries.push(ManifestEntry::Local {
            name: s.name.clone(),
            tarball_key: meta.tarball_key.clone(),
            sha256: meta.sha256.clone(),
            size_bytes: meta.size_bytes,
            description: s.description,
            uploaded_at: meta.uploaded_at.clone(),
        });
    }

    Manifest {
        version: Manifest::CURRENT_VERSION,
        generated_at,
        device_id,
        skills: entries,
    }
}

/// Look up every skill's `source_folder` from the on-disk lockfile (monorepo
/// case). Empty values are dropped.
fn load_source_folders() -> HashMap<String, String> {
    use skillstar_skills::lockfile;
    let lock_path = lockfile::lockfile_path();
    let lockfile = lockfile::Lockfile::load(&lock_path).unwrap_or_default();
    lockfile
        .skills
        .into_iter()
        .filter_map(|e| {
            e.source_folder
                .filter(|s| !s.is_empty())
                .map(|sf| (e.name, sf))
        })
        .collect()
}

/// Serialise a manifest to pretty JSON for upload.
pub fn serialise(manifest: &Manifest) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec_pretty(manifest)
}

/// Parse a downloaded manifest blob.
pub fn parse(bytes: &[u8]) -> Result<Manifest, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// Annotate each manifest entry with whether it is installed on this device,
/// producing the view returned to the UI by `pull_cloud_manifest`.
pub fn annotate_installed(manifest: Manifest) -> Vec<ManifestEntryView> {
    manifest
        .skills
        .into_iter()
        .map(|entry| {
            let installed_locally = is_installed_locally(entry.name());
            ManifestEntryView {
                entry,
                installed_locally,
            }
        })
        .collect()
}

fn is_installed_locally(name: &str) -> bool {
    let hub = skillstar_core::infra::paths::hub_skills_dir().join(name);
    hub.symlink_metadata().is_ok()
}

/// Summarise a push for the UI.
#[allow(clippy::too_many_arguments)]
pub fn summarise_push(
    hub_count: usize,
    local_count: usize,
    tarballs_uploaded: usize,
    tarballs_skipped: usize,
    manifest_uploaded: bool,
) -> PushSummary {
    PushSummary {
        hub_count,
        local_count,
        tarballs_uploaded,
        tarballs_skipped,
        manifest_uploaded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skillstar_core::types::{Skill, SkillCategory, SkillType};

    fn skill(name: &str, git: &str, st: SkillType) -> Skill {
        Skill {
            name: name.to_string(),
            description: format!("desc {name}"),
            localized_description: None,
            skill_type: st,
            stars: 0,
            installed: true,
            update_available: false,
            last_updated: "2026-01-01T00:00:00Z".to_string(),
            git_url: git.to_string(),
            tree_hash: Some("abc".to_string()),
            category: SkillCategory::None,
            author: None,
            topics: vec![],
            agent_links: Some(vec![]),
            rank: None,
            source: None,
        }
    }

    #[test]
    fn build_manifest_drops_hub_without_git_and_local_without_meta() {
        let _g = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }

        let hub = vec![
            skill("a", "https://x.git", SkillType::Hub),
            skill("b", "", SkillType::Hub), // dropped: no git_url
        ];
        let local = vec![
            skill("mine", "", SkillType::Local),
            skill("orphan", "", SkillType::Local), // dropped: no meta
        ];
        let mut meta = HashMap::new();
        meta.insert(
            "mine".to_string(),
            PackedLocal {
                sha256: "deadbeef".to_string(),
                size_bytes: 42,
                tarball_key: "tarballs/mine/deadbeef.tar.gz".to_string(),
                uploaded_at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        let m = build_manifest(hub, local, meta, "dev-1".to_string(), "2026-01-01T00:00:00Z".to_string());
        assert_eq!(m.skills.len(), 2);
        let kinds: Vec<&str> = m.skills.iter().map(|e| e.name()).collect();
        assert!(kinds.contains(&"a"));
        assert!(kinds.contains(&"mine"));
    }

    #[test]
    fn manifest_round_trip() {
        let _g = crate::test_support::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // SAFETY: held under env_lock() so concurrent tests can't race.
        unsafe {
            std::env::set_var("SKILLSTAR_DATA_DIR", dir.path());
        }

        let hub = vec![skill("a", "https://x.git", SkillType::Hub)];
        let local = vec![skill("mine", "", SkillType::Local)];
        let mut meta = HashMap::new();
        meta.insert(
            "mine".to_string(),
            PackedLocal {
                sha256: "deadbeef".to_string(),
                size_bytes: 42,
                tarball_key: "tarballs/mine/deadbeef.tar.gz".to_string(),
                uploaded_at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        let m = build_manifest(hub, local, meta, "dev-1".to_string(), "2026-01-01T00:00:00Z".to_string());
        let bytes = serialise(&m).unwrap();
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.skills.len(), 2);
        assert_eq!(parsed.device_id, "dev-1");
    }
}
