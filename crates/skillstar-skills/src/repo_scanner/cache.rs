use crate::discovery as skill_discover;
use crate::git::ops as git_ops;
use anyhow::{Context, Result};
use skillstar_core::infra::{path_env::command_with_path, paths};
use std::path::{Path, PathBuf};
use tracing::warn;

pub use crate::source_resolver::cache_dir_name;

pub fn clone_or_fetch_repo(repo_url: &str, source: &str) -> Result<PathBuf> {
    clone_or_fetch_repo_in_session(
        repo_url,
        source,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn clone_or_fetch_repo_in_session(
    repo_url: &str,
    source: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<PathBuf> {
    clone_or_fetch_repo_at_in_session(repo_url, source, None, session)
}

pub fn clone_or_fetch_repo_at(
    repo_url: &str,
    source: &str,
    git_ref: Option<&str>,
) -> Result<PathBuf> {
    clone_or_fetch_repo_at_in_session(
        repo_url,
        source,
        git_ref,
        &crate::git::transport::GitOperationSession::public(),
    )
}

pub fn clone_or_fetch_repo_at_in_session(
    repo_url: &str,
    source: &str,
    git_ref: Option<&str>,
    session: &crate::git::transport::GitOperationSession,
) -> Result<PathBuf> {
    let cache_dir = paths::repos_cache_dir();
    std::fs::create_dir_all(&cache_dir).context("Failed to create repo cache directory")?;

    let cache_key = match git_ref {
        Some(git_ref) => {
            validate_git_ref(git_ref)?;
            format!(
                "{}--ref--{}",
                cache_dir_name(source),
                cache_dir_name(git_ref)
            )
        }
        None => cache_dir_name(source),
    };
    let repo_dir = cache_dir.join(cache_key);

    if repo_dir.join(".git").exists() {
        if let Some(git_ref) = git_ref {
            fetch_and_reset_ref(&repo_dir, git_ref, session)?;
            return Ok(repo_dir);
        }

        git_ops::run_git_shallow_fetch_in_session(
            &repo_dir,
            &["fetch", "--depth", "1", "--quiet"],
            session,
        )
        .context("Failed to execute git fetch")?;

        git_ops::checkout_in_session(&repo_dir, &["reset", "--hard", "origin/HEAD"], session)
            .context("Failed to reset cached repository")?;

        if is_sparse_checkout(&repo_dir)
            && let Ok(dirs) = discover_skill_dirs_from_tree(&repo_dir)
        {
            if dirs.is_empty() {
                let _ = git_ops::checkout_in_session(
                    &repo_dir,
                    &["sparse-checkout", "disable"],
                    session,
                );
                let _ = git_ops::checkout_in_session(&repo_dir, &["checkout"], session);
            } else {
                let dir_refs: Vec<&str> = dirs.iter().map(|s| s.as_str()).collect();
                let _ = git_ops::apply_sparse_checkout_in_session(&repo_dir, &dir_refs, session);
            }
        }

        Ok(repo_dir)
    } else if let Some(git_ref) = git_ref {
        // Ref-pinned sources use an isolated cache entry. A shallow worktree is
        // intentionally preferred here: after fetching an arbitrary ref we
        // need its files available before applying any subpath filter.
        git_ops::clone_repo_shallow_in_session(repo_url, &repo_dir, session)
            .with_context(|| format!("Failed to shallow-clone {}", repo_url))?;
        fetch_and_reset_ref(&repo_dir, git_ref, session)?;
        Ok(repo_dir)
    } else {
        match clone_sparse_with_skills(repo_url, &repo_dir, session) {
            Ok(()) => Ok(repo_dir),
            Err(sparse_err) => {
                warn!(target: "repo_scanner", error = %sparse_err, "sparse clone failed, falling back to shallow");
                let _ = std::fs::remove_dir_all(&repo_dir);
                git_ops::clone_repo_shallow_in_session(repo_url, &repo_dir, session)
                    .with_context(|| format!("Failed to shallow-clone {}", repo_url))?;
                Ok(repo_dir)
            }
        }
    }
}

fn validate_git_ref(git_ref: &str) -> Result<()> {
    let invalid = git_ref.is_empty()
        || git_ref.starts_with('-')
        || git_ref.ends_with('/')
        || git_ref.ends_with('.')
        || git_ref.ends_with(".lock")
        || git_ref.contains("..")
        || git_ref.contains("//")
        || git_ref.contains("@{")
        || git_ref
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || "~^:?*[\\".contains(ch));
    if invalid {
        anyhow::bail!("Unsafe or invalid Git ref: {git_ref}");
    }
    Ok(())
}

fn fetch_and_reset_ref(
    repo_dir: &Path,
    git_ref: &str,
    session: &crate::git::transport::GitOperationSession,
) -> Result<()> {
    validate_git_ref(git_ref)?;

    git_ops::run_git_shallow_fetch_in_session(
        repo_dir,
        &["fetch", "--depth", "1", "--quiet", "origin", git_ref],
        session,
    )
    .with_context(|| format!("git fetch for ref '{git_ref}' failed"))?;

    git_ops::checkout_in_session(repo_dir, &["reset", "--hard", "FETCH_HEAD"], session)
        .with_context(|| format!("git checkout for ref '{git_ref}' failed"))?;

    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["config", "skillstar.ref", git_ref])
        .output()
        .context("Failed to persist requested Git ref in repo cache")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to persist Git ref '{git_ref}': {}", err.trim());
    }
    Ok(())
}

fn clone_sparse_with_skills(
    repo_url: &str,
    dest: &Path,
    session: &crate::git::transport::GitOperationSession,
) -> Result<()> {
    git_ops::clone_repo_sparse_in_session(repo_url, dest, session)?;

    let skill_dirs = discover_skill_dirs_from_tree(dest)?;

    if skill_dirs.is_empty() {
        let _ = git_ops::checkout_in_session(dest, &["checkout"], session);
        return Ok(());
    }

    let dir_refs: Vec<&str> = skill_dirs.iter().map(|s| s.as_str()).collect();
    git_ops::apply_sparse_checkout_in_session(dest, &dir_refs, session)?;

    Ok(())
}

pub(super) fn discover_skill_dirs_from_tree(repo_dir: &Path) -> Result<Vec<String>> {
    let all_paths = git_ops::list_tree_paths(repo_dir)?;
    Ok(derive_sparse_skill_dirs(&all_paths))
}

fn derive_sparse_skill_dirs(all_paths: &[String]) -> Vec<String> {
    if all_paths.iter().any(|p| p == "SKILL.md") {
        return Vec::new();
    }

    let skill_dirs: Vec<String> = all_paths
        .iter()
        .filter(|p| p.ends_with("/SKILL.md") || *p == "SKILL.md")
        .filter_map(|p| {
            let parent = Path::new(p).parent()?;
            let parent_str = parent.to_string_lossy().to_string();
            if parent_str.is_empty() {
                None
            } else {
                Some(parent_str)
            }
        })
        .collect();

    let mut canonical: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for dir in &skill_dirs {
        let skill_name = Path::new(dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if skill_name.is_empty() {
            continue;
        }

        let priority = skill_discover::source_priority(dir);
        let should_replace = canonical
            .get(&skill_name)
            .map(|existing| skill_discover::source_priority(existing) < priority)
            .unwrap_or(true);

        if should_replace {
            canonical.insert(skill_name, dir.clone());
        }
    }

    let mut result: Vec<String> = canonical.into_values().collect();
    result.sort();
    result.dedup();

    compact_to_common_parents(&result)
}

fn compact_to_common_parents(dirs: &[String]) -> Vec<String> {
    if dirs.is_empty() {
        return Vec::new();
    }

    let mut parent_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut parent_to_dirs: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for dir in dirs {
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            *parent_counts.entry(parent_str.clone()).or_insert(0) += 1;
            parent_to_dirs
                .entry(parent_str)
                .or_default()
                .push(dir.clone());
        }
    }

    let mut result = Vec::new();
    let mut handled = std::collections::HashSet::new();

    for dir in dirs {
        if handled.contains(dir) {
            continue;
        }
        if let Some(parent) = Path::new(dir).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            if parent_counts.get(&parent_str).copied().unwrap_or(0) >= 2 {
                if !handled.contains(&parent_str) {
                    result.push(parent_str.clone());
                    if let Some(children) = parent_to_dirs.get(&parent_str) {
                        for child in children {
                            handled.insert(child.clone());
                        }
                    }
                    handled.insert(parent_str);
                }
            } else {
                result.push(dir.clone());
                handled.insert(dir.clone());
            }
        } else {
            result.push(dir.clone());
            handled.insert(dir.clone());
        }
    }

    result.sort();
    result
}

pub(super) fn is_sparse_checkout(repo_dir: &Path) -> bool {
    let output = command_with_path("git")
        .current_dir(repo_dir)
        .args(["config", "--get", "core.sparseCheckout"])
        .output();
    match output {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim() == "true",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::compact_to_common_parents;

    #[test]
    fn compact_parents_groups_siblings() {
        let dirs = vec![
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
            "source/skills/bolder".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["source/skills"]);
    }

    #[test]
    fn compact_parents_preserves_singles() {
        let dirs = vec!["custom/my-skill".to_string()];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/my-skill"]);
    }

    #[test]
    fn compact_parents_mixed() {
        let dirs = vec![
            "custom/lone-skill".to_string(),
            "source/skills/adapt".to_string(),
            "source/skills/animate".to_string(),
        ];
        let compacted = compact_to_common_parents(&dirs);
        assert_eq!(compacted, vec!["custom/lone-skill", "source/skills"]);
    }

    #[test]
    fn compact_parents_empty() {
        let dirs: Vec<String> = Vec::new();
        let compacted = compact_to_common_parents(&dirs);
        assert!(compacted.is_empty());
    }
}
