//! Read-only project inspection: agent detection + skill scanning.

use anyhow::Result;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use super::index::load_index;
use super::store::load_skills_list;
use super::types::{
    AmbiguousGroup, DetectedAgent, ProjectAgentDetection, ProjectScanResult, ScannedSkill,
    ensure_project_root_exists,
};
use crate::agents as agent_profile;
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

    ProjectAgentDetection {
        detected,
        ambiguous_groups,
        auto_enable,
    }
}

/// Build the set of skills the project already manages, keyed by
/// `<project_skills_rel>::<skill_name>` so it lines up with how the scan keys
/// each discovered skill. Read-only: resolves the registered project name from
/// the index by path and reads its `skills-list.json` without mutating state.
///
/// A copy-deployed skill is a real directory (not a symlink) containing
/// SKILL.md, so it looks identical to an unmanaged skill on disk. The only
/// authoritative signal that SkillStar owns it is the manifest, hence this
/// comparison lives here in the backend rather than in the UI.
fn tracked_skill_keys(
    project_path: &str,
    profiles: &[agent_profile::AgentProfile],
) -> HashSet<String> {
    let Some(name) = load_index()
        .projects
        .into_iter()
        .find(|p| p.path == project_path)
        .map(|p| p.name)
    else {
        return HashSet::new();
    };
    let Some(list) = load_skills_list(&name) else {
        return HashSet::new();
    };

    // agent_id → project_skills_rel, so manifest entries can be keyed by the
    // shared on-disk path (several agents may target the same directory).
    let rel_by_agent: HashMap<&str, &str> = profiles
        .iter()
        .map(|p| (p.id.as_str(), p.project_skills_rel.as_str()))
        .collect();

    let mut keys = HashSet::new();
    for (agent_id, skill_names) in &list.agents {
        let rel = rel_by_agent
            .get(agent_id.as_str())
            .copied()
            .unwrap_or(agent_id.as_str());
        for skill_name in skill_names {
            keys.insert(format!("{rel}::{skill_name}"));
        }
    }
    keys
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
    let tracked = tracked_skill_keys(project_path, &profiles);

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

            let managed = tracked.contains(&format!("{}::{}", profile.project_skills_rel, name));

            skills.push(ScannedSkill {
                name,
                agent_id: profile.id.clone(),
                is_symlink,
                in_hub,
                has_skill_md,
                managed,
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
    ///  - universal agents share `.agents/skills` and require disambiguation
    ///  - OpenClaw uses its upstream project-level `skills` directory
    ///  - only the `skills` dir itself counts, never its parent
    #[test]
    fn detects_universal_agents_as_one_ambiguous_group() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join(".agents/skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        let codex = detection
            .detected
            .iter()
            .find(|d| d.agent_id == "codex")
            .expect("codex should be in the detection list");
        assert!(
            codex.exists,
            ".agents/skills exists → codex.exists must be true"
        );
        assert_eq!(codex.project_skills_rel, ".agents/skills");
        assert!(
            !detection.auto_enable.contains(&"codex".to_string()),
            "a shared universal path cannot choose an owner automatically"
        );
        let group = detection
            .ambiguous_groups
            .iter()
            .find(|group| group.path == ".agents/skills")
            .expect("universal path should be ambiguous");
        assert!(group.agent_ids.contains(&"codex".to_string()));
        assert!(group.agent_ids.contains(&"cursor".to_string()));
    }

    #[test]
    fn does_not_detect_agent_when_only_parent_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        // Only `.agents` exists, NOT `.agents/skills` — must not be detected,
        // matching AGENTS.md ("strictly on the skills dir itself, never its parent").
        std::fs::create_dir_all(project.join(".agents")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        let codex = detection
            .detected
            .iter()
            .find(|d| d.agent_id == "codex")
            .expect("codex should still be listed (with exists=false)");
        assert!(
            !codex.exists,
            ".agents without skills/ must NOT count as detected"
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
        std::fs::create_dir_all(project.join(".agents/skills")).unwrap();
        std::fs::create_dir_all(project.join(".claude/skills")).unwrap();
        std::fs::create_dir_all(project.join(".qoder/skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        assert!(
            detection
                .ambiguous_groups
                .iter()
                .any(|group| group.path == ".agents/skills")
        );
        assert!(detection.auto_enable.contains(&"claude".to_string()));
        assert!(
            detection
                .ambiguous_groups
                .iter()
                .any(|group| group.path == ".qoder/skills"
                    && group.agent_ids.contains(&"qoder".to_string())
                    && group.agent_ids.contains(&"qoder-cn".to_string())),
            "Qoder and Qoder CN share the same upstream project path"
        );
    }

    #[test]
    fn openclaw_project_skills_directory_is_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        std::fs::create_dir_all(project.join("skills")).unwrap();

        let detection = detect_project_agents(&project.to_string_lossy());
        assert!(
            detection
                .detected
                .iter()
                .any(|d| d.agent_id == "openclaw" && d.exists),
            "OpenClaw's upstream project skills dir should be detected"
        );
        assert!(
            detection.auto_enable.contains(&"openclaw".to_string()),
            "OpenClaw owns a unique project path"
        );
    }
}
