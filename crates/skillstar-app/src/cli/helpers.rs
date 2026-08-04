//! Portable CLI helper functions.
//!
//! These helpers are pure enough to live in the skillstar-cli crate and
//! depend only on extracted crates (skillstar-infra, skillstar-core-types,
//! skillstar-skills, skillstar-skill-core).

use std::io::{self, IsTerminal, Write};
use std::path::Path;

use skillstar_core::infra::paths::{hub_skills_dir, lockfile_path};
use skillstar_skills::agents::list_profiles;
use skillstar_skills::lockfile::Lockfile;
use skillstar_skills::source_resolver::same_remote_url;

// ── Name resolution helpers ─────────────────────────────────────────────

/// Derive a skill name hint from a Git URL.
pub fn derive_name_hint(url: &str, explicit_name: Option<&str>) -> String {
    explicit_name.map(str::to_string).unwrap_or_else(|| {
        if let Ok(source) = skillstar_skills::source_resolver::Source::parse(url) {
            if let Some(skill) = source.skill_filter {
                return skill;
            }
            if let Some(subpath) = source.subpath
                && let Some(name) = subpath.rsplit('/').next()
            {
                return name.to_string();
            }
            if let Some(name) = source.short.rsplit('/').next() {
                return name.to_string();
            }
        }

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
        .map(|id| match id.trim().to_lowercase().as_str() {
            // Accept the canonical ids documented by vercel-labs/skills while
            // preserving SkillStar's existing persisted profile ids.
            "claude-code" => "claude".to_string(),
            "kiro-cli" => "kiro".to_string(),
            "hermes-agent" => "hermes".to_string(),
            normalized => normalized.to_string(),
        })
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

pub fn supported_agent_ids(global: bool) -> Vec<String> {
    let mut supported = list_profiles()
        .into_iter()
        .filter(|profile| {
            if global {
                profile.has_global_skills()
            } else {
                profile.has_project_skills()
            }
        })
        .map(|profile| profile.id)
        .collect::<Vec<_>>();
    supported.sort();
    supported.dedup();
    supported
}

pub fn prompt_for_agent_selection_for_scope(
    enabled_agent_ids: &[String],
    global: bool,
) -> Vec<String> {
    if !io::stdin().is_terminal() {
        return enabled_agent_ids.to_vec();
    }

    let supported_ids = supported_agent_ids(global);
    if supported_ids.is_empty() {
        return enabled_agent_ids.to_vec();
    }
    let default_text = if enabled_agent_ids.is_empty() {
        "manual selection".to_string()
    } else {
        format!("Settings-enabled ({})", enabled_agent_ids.join(", "))
    };

    println!(
        "Select target agent(s) for {} install:",
        if global { "global" } else { "project" }
    );
    println!("  Available: {}", supported_ids.join(", "));
    println!(
        "  Press Enter for {} or input comma-separated agent ids.",
        default_text
    );
    print!("  Agent(s): ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return enabled_agent_ids.to_vec();
    }

    let input = input.trim();
    if input.is_empty() {
        return enabled_agent_ids.to_vec();
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
    validate_agent_ids_for_scope(agent_ids, false)
}

pub fn validate_agent_ids_for_scope(
    agent_ids: &[String],
    global: bool,
) -> Result<Vec<String>, String> {
    let normalized = normalize_agent_ids(agent_ids);
    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    let supported_ids = supported_agent_ids(global);
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

/// Print project target paths for a skill, annotated with actual deploy kind.
pub fn print_project_targets(project_path: &Path, rel_dirs: &[String], skill_name: &str) {
    for rel_dir in rel_dirs {
        let linked_path = project_path.join(rel_dir).join(skill_name);
        let kind = describe_deploy_kind(&linked_path);
        println!("  ↳ {}  [{}]", linked_path.display(), kind);
    }
}

fn describe_deploy_kind(path: &Path) -> &'static str {
    if path.symlink_metadata().is_err() {
        return "missing";
    }
    if skillstar_core::infra::fs_ops::is_link(path) {
        if path.exists() {
            return "link";
        }
        return "broken-link";
    }
    if path.is_dir() {
        return "copy";
    }
    "unknown"
}

/// Resolve the Agents the user has manually enabled in Settings.
pub fn resolve_enabled_agents(global: bool) -> Vec<String> {
    let mut agent_ids: Vec<String> = list_profiles()
        .into_iter()
        .filter(|profile| profile.enabled)
        .filter(|profile| {
            if global {
                profile.has_global_skills()
            } else {
                profile.has_project_skills()
            }
        })
        .map(|profile| profile.id)
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
            "my-skill"
        );
        assert_eq!(
            derive_name_hint(
                "https://github.com/user/my-skill/tree/main/skills/react",
                None
            ),
            "react"
        );
        assert_eq!(derive_name_hint("user/my-skill@selected", None), "selected");
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

    #[test]
    fn test_normalize_agent_ids_accepts_upstream_names() {
        let ids = vec![
            "claude-code".to_string(),
            "kiro-cli".to_string(),
            "hermes-agent".to_string(),
            "antigravity-cli".to_string(),
        ];
        assert_eq!(
            normalize_agent_ids(&ids),
            vec!["antigravity-cli", "claude", "hermes", "kiro"]
        );
    }
}
