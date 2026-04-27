//! Portable CLI helper functions.
//!
//! These helpers are pure enough to live in the skillstar-cli crate and
//! depend only on extracted crates (skillstar-infra, skillstar-core-types,
//! skillstar-projects, skillstar-skill-core).

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use skillstar_core_types::lockfile::Lockfile;
use skillstar_infra::paths::{hub_skills_dir, lockfile_path};
use skillstar_projects::agents::list_profiles;
use skillstar_projects::project_manifest::detect_project_agents;
use skillstar_skill_core::source_resolver::same_remote_url;

// ── Name resolution helpers ─────────────────────────────────────────────

/// Derive a skill name hint from a Git URL.
pub fn derive_name_hint(url: &str, explicit_name: Option<&str>) -> String {
    explicit_name.map(str::to_string).unwrap_or_else(|| {
        url.rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    })
}

/// Resolve the installed skill name for a given URL.
pub fn resolve_installed_name(
    url: &str,
    explicit_name: Option<&str>,
    name_hint: &str,
) -> Result<Option<String>, String> {
    let skills_dir = hub_skills_dir();
    let lock_path = lockfile_path();
    let lockfile = Lockfile::load(&lock_path).unwrap_or_default();
    let has_matching_lock = |name: &str| {
        lockfile
            .skills
            .iter()
            .any(|entry| entry.name == name && same_remote_url(&entry.git_url, url))
    };

    if let Some(name) = explicit_name {
        if skills_dir.join(name).exists() {
            if has_matching_lock(name) {
                return Ok(Some(name.to_string()));
            }
            return Err(format!(
                "Skill '{}' already exists but is linked to a different repository. Re-run with a different --name or uninstall the existing skill first.",
                name
            ));
        }
    } else if skills_dir.join(name_hint).exists() && has_matching_lock(name_hint) {
        return Ok(Some(name_hint.to_string()));
    }

    let mut matches: Vec<String> = lockfile
        .skills
        .iter()
        .filter(|entry| {
            same_remote_url(&entry.git_url, url) && skills_dir.join(&entry.name).exists()
        })
        .map(|entry| entry.name.clone())
        .collect();
    matches.sort();
    matches.dedup();

    if let Some(name) = explicit_name {
        if matches.iter().any(|candidate| candidate == name) {
            return Ok(Some(name.to_string()));
        }
        return Ok(None);
    }

    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(format!(
            "Repository '{}' maps to multiple installed skills ({}). Please re-run with --name <skill-name>.",
            url,
            matches.join(", ")
        )),
    }
}

// ── Agent ID helpers ────────────────────────────────────────────────────

/// Normalize a list of agent IDs (lowercase, trim, sort, dedup).
pub fn normalize_agent_ids(agent_ids: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = agent_ids
        .iter()
        .map(|id| id.trim().to_lowercase())
        .filter(|id| !id.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    normalized
}

/// List supported project agents as (id, project_skills_rel) pairs.
pub fn supported_project_agents() -> Vec<(String, String)> {
    let mut supported: Vec<(String, String)> = list_profiles()
        .into_iter()
        .filter(|profile| profile.has_project_skills())
        .map(|profile| (profile.id, profile.project_skills_rel))
        .collect();
    supported.sort_by(|a, b| a.0.cmp(&b.0));
    supported.dedup_by(|a, b| a.0 == b.0);
    supported
}

/// Prompt the user to select agent IDs interactively.
pub fn prompt_for_agent_selection(auto_agent_ids: &[String]) -> Vec<String> {
    if !io::stdin().is_terminal() {
        return auto_agent_ids.to_vec();
    }

    let supported = supported_project_agents();
    if supported.is_empty() {
        return auto_agent_ids.to_vec();
    }

    let supported_ids: Vec<String> = supported.into_iter().map(|(id, _)| id).collect();
    let default_text = if auto_agent_ids.is_empty() {
        "auto fallback (.agents/skills)".to_string()
    } else {
        format!("auto detected ({})", auto_agent_ids.join(", "))
    };

    println!("Select target agent(s) for project link:");
    println!("  Available: {}", supported_ids.join(", "));
    println!(
        "  Press Enter for {} or input comma-separated agent ids.",
        default_text
    );
    print!("  Agent(s): ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return auto_agent_ids.to_vec();
    }

    let input = input.trim();
    if input.is_empty() {
        return auto_agent_ids.to_vec();
    }

    let parsed: Vec<String> = input
        .split(',')
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    normalize_agent_ids(&parsed)
}

/// Validate agent IDs against supported project agents.
pub fn validate_agent_ids(agent_ids: &[String]) -> Result<Vec<String>, String> {
    let normalized = normalize_agent_ids(agent_ids);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let supported = supported_project_agents();
    let supported_ids: Vec<String> = supported.iter().map(|(id, _)| id.clone()).collect();
    let mut invalid = Vec::new();
    for agent_id in &normalized {
        if !supported_ids.iter().any(|id| id == agent_id) {
            invalid.push(agent_id.clone());
        }
    }

    if invalid.is_empty() {
        Ok(normalized)
    } else {
        Err(format!(
            "Unknown agent id(s): {}. Supported agents: {}.",
            invalid.join(", "),
            supported_ids.join(", ")
        ))
    }
}

/// Resolve relative skill directories for the given agent IDs.
pub fn resolve_rel_dirs_for_agents(agent_ids: &[String]) -> Vec<String> {
    if agent_ids.is_empty() {
        return vec![".agents/skills".to_string()];
    }

    let supported = supported_project_agents();
    let mut rel_dirs = Vec::new();
    for agent_id in agent_ids {
        if let Some((_, rel_dir)) = supported.iter().find(|(id, _)| id == agent_id) {
            rel_dirs.push(rel_dir.clone());
        }
    }

    rel_dirs.sort();
    rel_dirs.dedup();
    if rel_dirs.is_empty() {
        rel_dirs.push(".agents/skills".to_string());
    }
    rel_dirs
}

/// Print project target paths for a skill.
pub fn print_project_targets(project_path: &Path, rel_dirs: &[String], skill_name: &str) {
    for rel_dir in rel_dirs {
        let linked_path = project_path.join(rel_dir).join(skill_name);
        println!("  ↳ {}", linked_path.display());
    }
}

/// Auto-detect project agents from a project directory.
pub fn resolve_auto_project_agents(project_path: &Path) -> Vec<String> {
    let detection = detect_project_agents(&project_path.to_string_lossy());
    let mut agent_ids: Vec<String> = detection
        .detected
        .iter()
        .filter(|agent| agent.exists)
        .map(|agent| agent.agent_id.clone())
        .collect();
    agent_ids.sort();
    agent_ids.dedup();
    agent_ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_name_hint_from_url() {
        assert_eq!(
            derive_name_hint("https://github.com/user/my-skill", None),
            "my-skill"
        );
        assert_eq!(
            derive_name_hint("https://github.com/user/my-skill.git", None),
            "my-skill"
        );
        assert_eq!(
            derive_name_hint("https://github.com/user/my-skill/tree/main", None),
            "main"
        );
    }

    #[test]
    fn test_derive_name_hint_explicit_name() {
        assert_eq!(
            derive_name_hint("https://github.com/user/repo", Some("custom-name")),
            "custom-name"
        );
    }

    #[test]
    fn test_derive_name_hint_no_slashes() {
        assert_eq!(derive_name_hint("bare-reponame", None), "bare-reponame");
    }

    #[test]
    fn test_normalize_agent_ids_empty() {
        let ids: [String; 0] = [];
        assert!(normalize_agent_ids(&ids).is_empty());
    }

    #[test]
    fn test_normalize_agent_ids_trims_and_lowercases() {
        let ids = vec!["  CODEX  ".to_string(), " Claude ".to_string()];
        let normalized = normalize_agent_ids(&ids);
        assert_eq!(normalized, vec!["claude", "codex"]);
    }

    #[test]
    fn test_normalize_agent_ids_removes_empty() {
        let ids = vec!["codex".to_string(), "".to_string(), "  ".to_string()];
        assert_eq!(normalize_agent_ids(&ids), vec!["codex"]);
    }

    #[test]
    fn test_normalize_agent_ids_sorts_and_dedups() {
        let ids = vec!["zulu".to_string(), "alpha".to_string(), "zulu".to_string()];
        assert_eq!(normalize_agent_ids(&ids), vec!["alpha", "zulu"]);
    }
}
