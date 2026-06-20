//! Read-only project inspection: agent detection + skill scanning.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::types::{
    AmbiguousGroup, DetectedAgent, ProjectAgentDetection, ProjectScanResult, ScannedSkill,
    ensure_project_root_exists,
};
use crate::projects::agents as agent_profile;
use skillstar_core::infra::{fs_ops, paths as fs_paths};

/// Scan a project directory for existing agent skill directories.
///
/// For each agent profile, check if `<project_root>/<project_skills_rel>` exists.
/// Unique paths that exist → auto-enable. Shared paths that exist → ambiguous group.
pub fn detect_project_agents(project_path: &str) -> ProjectAgentDetection {
    let profiles = agent_profile::list_profiles();
    let project = Path::new(project_path);

    // Build detection list
    let detected: Vec<DetectedAgent> = profiles
        .iter()
        .filter(|p| p.has_project_skills())
        .map(|p| {
            let skills_dir = project.join(&p.project_skills_rel);
            DetectedAgent {
                agent_id: p.id.clone(),
                display_name: p.display_name.clone(),
                icon: p.icon.clone(),
                project_skills_rel: p.project_skills_rel.clone(),
                exists: skills_dir.exists(),
            }
        })
        .collect();

    // Group agents by project_skills_rel
    let mut path_groups: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for d in &detected {
        if d.exists {
            path_groups
                .entry(d.project_skills_rel.clone())
                .or_default()
                .push((d.agent_id.clone(), d.display_name.clone()));
        }
    }

    let mut ambiguous_groups = Vec::new();
    let mut auto_enable = Vec::new();

    for (path, agents) in &path_groups {
        if agents.len() > 1 {
            ambiguous_groups.push(AmbiguousGroup {
                path: path.clone(),
                agent_ids: agents.iter().map(|(id, _)| id.clone()).collect(),
                agent_names: agents.iter().map(|(_, name)| name.clone()).collect(),
            });
        } else if let Some((id, _)) = agents.first() {
            auto_enable.push(id.clone());
        }
    }

    // Disambiguation sealed — each agent now has a unique project_skills_rel,
    // so ambiguous groups can no longer occur. The detection logic above is
    // preserved but its output is discarded.
    ProjectAgentDetection {
        detected,
        ambiguous_groups: Vec::new(),
        auto_enable,
    }
}

/// Scan a project directory for existing skill entries across all agent profiles.
///
/// For each agent profile, look at `<project>/<project_skills_rel>/` and inspect
/// every child directory. Classify each as symlink vs real directory, check
/// whether the hub already has a skill with the same name, and whether the
/// directory contains a SKILL.md file.
pub fn scan_project_skills(project_path: &str) -> Result<ProjectScanResult> {
    let hub_dir = fs_paths::hub_skills_dir();
    let profiles = agent_profile::list_profiles();
    let project = ensure_project_root_exists(project_path)?;

    let mut skills = Vec::with_capacity(profiles.len() * 8); // reasonable pre-alloc
    let mut agents_found = Vec::with_capacity(profiles.len());

    for profile in &profiles {
        if !profile.has_project_skills() {
            continue;
        }
        let skills_dir = project.join(&profile.project_skills_rel);
        let entries = match std::fs::read_dir(&skills_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut found_any = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() && !fs_ops::is_link(&path) {
                continue;
            }
            // For symlinks that point to directories, is_dir() returns true
            // but we also need is_symlink() check
            let is_symlink = fs_ops::is_link(&path);
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }

            let in_hub = hub_dir.join(&name).exists();
            let has_skill_md = path.join("SKILL.md").exists();
            if !is_symlink && !has_skill_md {
                continue;
            }

            skills.push(ScannedSkill {
                name,
                agent_id: profile.id.clone(),
                is_symlink,
                in_hub,
                has_skill_md,
            });
            found_any = true;
        }

        if found_any {
            agents_found.push(profile.id.clone());
        }
    }

    Ok(ProjectScanResult {
        skills,
        agents_found,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `detect_project_agents` is the read-only data-driven detector behind the
    /// Tauri command of the same name. AGENTS.md mandates:
    ///  - unique `project_skills_rel` per agent (disambiguation sealed)
    ///  - OpenClaw is global-only (`project_skills_rel` empty → never detected)
    ///  - only the `skills` dir itself counts, never its parent
    #[test]
    fn detects_codex_when_skills_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join(".codex/skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        let codex = detection
            .detected
            .iter()
            .find(|d| d.agent_id == "codex")
            .expect("codex should be in the detection list");
        assert!(codex.exists, ".codex/skills exists → codex.exists must be true");
        assert_eq!(codex.project_skills_rel, ".codex/skills");
        // Unique path → auto-enabled, no ambiguous group.
        assert!(
            detection.auto_enable.contains(&"codex".to_string()),
            "codex should be auto-enabled"
        );
        assert!(
            detection.ambiguous_groups.is_empty(),
            "sealed disambiguation must yield no ambiguous groups"
        );
    }

    #[test]
    fn does_not_detect_agent_when_only_parent_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        // Only `.codex` exists, NOT `.codex/skills` — must not be detected,
        // matching AGENTS.md ("strictly on the skills dir itself, never its parent").
        std::fs::create_dir_all(project.join(".codex")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        let codex = detection
            .detected
            .iter()
            .find(|d| d.agent_id == "codex")
            .expect("codex should still be listed (with exists=false)");
        assert!(
            !codex.exists,
            ".codex without skills/ must NOT count as detected"
        );
        assert!(
            !detection.auto_enable.contains(&"codex".to_string()),
            "codex must not be auto-enabled without its skills dir"
        );
    }

    #[test]
    fn detects_multiple_distinct_agents_simultaneously() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join(".codex/skills")).unwrap();
        std::fs::create_dir_all(project.join(".claude/skills")).unwrap();
        std::fs::create_dir_all(project.join(".gemini/skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        // Three distinct, unique rels → all auto-enabled, zero ambiguity.
        assert_eq!(detection.auto_enable.len(), 3);
        assert!(detection.ambiguous_groups.is_empty());
        for id in &["codex", "claude", "gemini"] {
            assert!(
                detection.auto_enable.contains(&id.to_string()),
                "{id} should be auto-enabled"
            );
        }
    }

    /// OpenClaw has an empty `project_skills_rel` (global-only), so it must
    /// never appear in the detected list regardless of directory state.
    #[test]
    fn openclaw_is_never_detected_at_project_level() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        // Even if ~/.openclaw/skills exists globally, the project detector
        // only looks at project-relative dirs, and openclaw has no rel.
        std::fs::create_dir_all(project.join(".openclaw/skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        assert!(
            !detection.detected.iter().any(|d| d.agent_id == "openclaw"),
            "openclaw must not appear in project-level detection (global-only)"
        );
        assert!(
            !detection.auto_enable.contains(&"openclaw".to_string()),
            "openclaw must not be auto-enabled"
        );
    }
}
