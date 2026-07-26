//! URL normalization for repository sources.
//!
//! Converts user-provided repository identifiers into normalized clone URLs
//! and short display identifiers.

use anyhow::{Result, anyhow};

// ── Source type ─────────────────────────────────────────────────────

/// Normalized source identifier for a skill repository.
///
/// `Source::parse` is the primary entry point for parsing user-provided
/// repository inputs (owner/repo, HTTPS URLs, .git-suffixed URLs).
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// Full clone URL, e.g. `https://github.com/owner/repo.git`
    pub repo_url: String,
    /// Short identifier, e.g. `owner/repo`
    pub short: String,
    /// Optional Git ref selected by a tree URL or `#ref` suffix.
    pub git_ref: Option<String>,
    /// Optional repository-relative directory to scan.
    pub subpath: Option<String>,
    /// Optional skill selected by `owner/repo@skill` or `#ref@skill`.
    pub skill_filter: Option<String>,
}

impl Source {
    /// Parse any accepted input into a normalized `Source`.
    ///
    /// Accepts the same common Git forms as `npx skills add`:
    /// - `owner/repo`, `owner/repo/path`, `owner/repo@skill`
    /// - GitHub / GitLab repository and tree URLs
    /// - HTTPS, SCP-style SSH, and `ssh://` Git URLs
    /// - an optional `#ref` or `#ref@skill` suffix
    ///
    /// Errors on empty or whitespace input.
    pub fn parse(input: &str) -> Result<Source> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Repository URL cannot be empty"));
        }

        let (source_without_fragment, fragment_ref, fragment_skill) = split_fragment(trimmed)?;
        let source_without_fragment = source_without_fragment
            .strip_prefix("github:")
            .map(str::to_string)
            .unwrap_or_else(|| source_without_fragment.to_string());

        if let Some(gitlab) = source_without_fragment.strip_prefix("gitlab:") {
            return parse_http_source(
                &format!("https://gitlab.com/{gitlab}"),
                fragment_ref,
                fragment_skill,
            );
        }

        if source_without_fragment.starts_with("git@")
            || source_without_fragment.starts_with("ssh://")
        {
            let short = short_from_ssh(&source_without_fragment)?;
            return Ok(Source {
                repo_url: source_without_fragment,
                short,
                git_ref: fragment_ref,
                subpath: None,
                skill_filter: fragment_skill,
            });
        }

        if source_without_fragment.starts_with("http://")
            || source_without_fragment.starts_with("https://")
        {
            return parse_http_source(&source_without_fragment, fragment_ref, fragment_skill);
        }

        parse_github_shorthand(&source_without_fragment, fragment_ref, fragment_skill)
    }
}

fn split_fragment(input: &str) -> Result<(String, Option<String>, Option<String>)> {
    let Some((source, fragment)) = input.split_once('#') else {
        return Ok((input.to_string(), None, None));
    };
    if fragment.trim().is_empty() {
        return Err(anyhow!("Git ref after '#' cannot be empty"));
    }
    let (git_ref, skill_filter) = match fragment.split_once('@') {
        Some((git_ref, skill)) => (git_ref, Some(skill)),
        None => (fragment, None),
    };
    if git_ref.trim().is_empty() {
        return Err(anyhow!("Git ref after '#' cannot be empty"));
    }
    let skill_filter = skill_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok((
        source.to_string(),
        Some(git_ref.trim().to_string()),
        skill_filter,
    ))
}

fn parse_github_shorthand(
    input: &str,
    git_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<Source> {
    if input.contains(':') || input.starts_with('.') || input.starts_with('/') {
        return Err(anyhow!(
            "Invalid repository format. Use 'owner/repo', a Git URL, or a local path."
        ));
    }

    let (path, inline_skill) = match input.rsplit_once('@') {
        Some((path, skill)) if path.split('/').count() == 2 => {
            (path, Some(skill.trim().to_string()))
        }
        _ => (input, None),
    };
    let parts = path
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(anyhow!(
            "Invalid repository format. Use 'owner/repo' or a full Git URL."
        ));
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return Err(anyhow!("Repository owner and name cannot be empty"));
    }

    let subpath = if parts.len() > 2 {
        Some(sanitize_subpath(&parts[2..].join("/"))?)
    } else {
        None
    };
    let short = format!("{owner}/{repo}");
    Ok(Source {
        repo_url: format!("https://github.com/{short}.git"),
        short,
        git_ref,
        subpath,
        skill_filter: fragment_skill.or(inline_skill),
    })
}

fn parse_http_source(
    input: &str,
    fragment_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<Source> {
    let (scheme, remainder) = input
        .split_once("://")
        .ok_or_else(|| anyhow!("Invalid Git URL: {input}"))?;
    let (authority, raw_path) = remainder
        .split_once('/')
        .ok_or_else(|| anyhow!("Git URL must contain a repository path: {input}"))?;
    let path = raw_path.trim_end_matches('/');
    if authority.is_empty() || path.is_empty() {
        return Err(anyhow!("Git URL must contain a host and repository path"));
    }

    if authority.eq_ignore_ascii_case("github.com") {
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(anyhow!("GitHub URL must contain owner/repo"));
        }
        let owner = parts[0];
        let repo = parts[1].trim_end_matches(".git");
        let mut git_ref = fragment_ref;
        let mut subpath = None;
        if parts.get(2).is_some_and(|part| *part == "tree") {
            let tree_ref = parts
                .get(3)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("GitHub tree URL must include a ref"))?;
            git_ref = Some((*tree_ref).to_string());
            if parts.len() > 4 {
                subpath = Some(sanitize_subpath(&parts[4..].join("/"))?);
            }
        }
        let short = format!("{owner}/{repo}");
        return Ok(Source {
            repo_url: format!("{scheme}://{authority}/{short}.git"),
            short,
            git_ref,
            subpath,
            skill_filter: fragment_skill,
        });
    }

    if let Some((repo_path, tree_path)) = path.split_once("/-/tree/") {
        let mut tree_parts = tree_path.split('/');
        let tree_ref = tree_parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("GitLab tree URL must include a ref"))?;
        let remaining = tree_parts.collect::<Vec<_>>().join("/");
        let clean_repo_path = repo_path.trim_end_matches(".git");
        return Ok(Source {
            repo_url: format!("{scheme}://{authority}/{clean_repo_path}.git"),
            short: clean_repo_path.to_string(),
            git_ref: Some(tree_ref.to_string()).or(fragment_ref),
            subpath: if remaining.is_empty() {
                None
            } else {
                Some(sanitize_subpath(&remaining)?)
            },
            skill_filter: fragment_skill,
        });
    }

    let clean_path = path.trim_end_matches(".git");
    let repo_url = if path.ends_with(".git") {
        input.to_string()
    } else {
        format!("{scheme}://{authority}/{clean_path}.git")
    };
    Ok(Source {
        repo_url,
        short: format!("{authority}/{clean_path}"),
        git_ref: fragment_ref,
        subpath: None,
        skill_filter: fragment_skill,
    })
}

fn short_from_ssh(input: &str) -> Result<String> {
    let path = if let Some(rest) = input.strip_prefix("ssh://") {
        rest.split_once('/')
            .map(|(_, path)| path)
            .ok_or_else(|| anyhow!("SSH URL must contain a repository path"))?
    } else {
        input
            .split_once(':')
            .map(|(_, path)| path)
            .ok_or_else(|| anyhow!("SSH URL must use git@host:path syntax"))?
    };
    let short = path.trim_matches('/').trim_end_matches(".git");
    if !short.contains('/') {
        return Err(anyhow!("SSH URL must contain owner/repo"));
    }
    Ok(short.to_string())
}

fn sanitize_subpath(input: &str) -> Result<String> {
    let normalized = input.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|segment| segment == ".." || segment == ".")
    {
        return Err(anyhow!(
            "Unsafe repository subpath '{input}': relative traversal is not allowed"
        ));
    }
    let normalized = normalized.trim_matches('/');
    if normalized.is_empty() {
        return Err(anyhow!("Repository subpath cannot be empty"));
    }
    Ok(normalized.to_string())
}

// ── Input Classification ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InputKind {
    Url(String),
    OwnerRepo { owner: String, repo: String },
    SkillName(String),
}

pub fn classify_input(input: &str) -> InputKind {
    let trimmed = input.trim();

    if trimmed.to_lowercase().starts_with("https://")
        || trimmed.to_lowercase().starts_with("http://")
    {
        return InputKind::Url(trimmed.to_string());
    }

    if trimmed.contains('/') {
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return InputKind::OwnerRepo {
                owner: parts[0].to_string(),
                repo: parts[1].trim_end_matches(".git").to_string(),
            };
        }
        return InputKind::Url(trimmed.to_string());
    }

    InputKind::SkillName(trimmed.to_string())
}

// ── URL Normalization ───────────────────────────────────────────────

/// Normalize a repository URL or owner/repo shorthand.
///
/// Returns `(repo_url, short)` where:
/// - `repo_url` is the full `.git`-suffixed clone URL
/// - `short` is the `owner/repo` identifier
///
/// This function is a compatibility shim that delegates to `Source::parse`.
pub fn normalize_repo_url(input: &str) -> Result<(String, String)> {
    let source = Source::parse(input)?;
    Ok((source.repo_url, source.short))
}

pub fn cache_dir_name(source: &str) -> String {
    source
        .chars()
        .flat_map(|ch| {
            if ch == '/' || ch == '\\' {
                "--".chars().collect::<Vec<_>>()
            } else if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                vec![ch]
            } else {
                vec!['-']
            }
        })
        .collect()
}

pub fn same_remote_url(left: &str, right: &str) -> bool {
    let left = Source::parse(left)
        .map(|source| source.repo_url)
        .unwrap_or_else(|_| left.to_string());
    let right = Source::parse(right)
        .map(|source| source.repo_url)
        .unwrap_or_else(|_| right.to_string());
    normalize_remote_url(&left) == normalize_remote_url(&right)
}

pub fn normalize_remote_url(url: &str) -> String {
    url.trim()
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_owner_repo() {
        let (url, source) = normalize_repo_url("vercel-labs/skills").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_full_url() {
        let (url, source) = normalize_repo_url("https://github.com/vercel-labs/skills").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_full_url_with_git_suffix() {
        let (url, source) =
            normalize_repo_url("https://github.com/vercel-labs/skills.git").unwrap();
        assert_eq!(url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(source, "vercel-labs/skills");
    }

    #[test]
    fn normalize_empty_input_fails() {
        assert!(normalize_repo_url("").is_err());
        assert!(normalize_repo_url("  ").is_err());
    }

    #[test]
    fn same_remote_url_matches() {
        assert!(same_remote_url(
            "https://github.com/owner/repo.git",
            "https://github.com/owner/repo",
        ));
        assert!(same_remote_url(
            "https://github.com/Owner/Repo",
            "https://github.com/owner/repo",
        ));
    }

    #[test]
    fn same_remote_url_different_repos() {
        assert!(!same_remote_url(
            "https://github.com/owner/repo-a",
            "https://github.com/owner/repo-b",
        ));
    }

    // Source::parse tests

    #[test]
    fn source_parse_owner_repo() {
        let s = Source::parse("vercel-labs/skills").unwrap();
        assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(s.short, "vercel-labs/skills");
    }

    #[test]
    fn source_parse_full_url() {
        let s = Source::parse("https://github.com/vercel-labs/skills").unwrap();
        assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(s.short, "vercel-labs/skills");
    }

    #[test]
    fn source_parse_url_with_git_suffix() {
        let s = Source::parse("https://github.com/vercel-labs/skills.git").unwrap();
        assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(s.short, "vercel-labs/skills");
    }

    #[test]
    fn source_parse_url_with_trailing_slash() {
        let s = Source::parse("https://github.com/vercel-labs/skills/").unwrap();
        assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(s.short, "vercel-labs/skills");
    }

    #[test]
    fn source_parse_owner_repo_with_git_suffix() {
        let s = Source::parse("owner/repo.git").unwrap();
        assert_eq!(s.repo_url, "https://github.com/owner/repo.git");
        assert_eq!(s.short, "owner/repo");
    }

    #[test]
    fn source_parse_shorthand_subpath_and_inline_skill() {
        let subtree = Source::parse("owner/repo/skills/react").unwrap();
        assert_eq!(subtree.repo_url, "https://github.com/owner/repo.git");
        assert_eq!(subtree.subpath.as_deref(), Some("skills/react"));
        assert_eq!(subtree.skill_filter, None);

        let selected = Source::parse("owner/repo@react").unwrap();
        assert_eq!(selected.repo_url, "https://github.com/owner/repo.git");
        assert_eq!(selected.subpath, None);
        assert_eq!(selected.skill_filter.as_deref(), Some("react"));
    }

    #[test]
    fn source_parse_github_tree_url() {
        let s = Source::parse(
            "https://github.com/vercel-labs/skills/tree/canary/skills/react-best-practices",
        )
        .unwrap();
        assert_eq!(s.repo_url, "https://github.com/vercel-labs/skills.git");
        assert_eq!(s.short, "vercel-labs/skills");
        assert_eq!(s.git_ref.as_deref(), Some("canary"));
        assert_eq!(s.subpath.as_deref(), Some("skills/react-best-practices"));
    }

    #[test]
    fn source_parse_gitlab_tree_and_ssh_ref() {
        let gitlab =
            Source::parse("https://gitlab.com/group/subgroup/repo/-/tree/release/skills/demo")
                .unwrap();
        assert_eq!(
            gitlab.repo_url,
            "https://gitlab.com/group/subgroup/repo.git"
        );
        assert_eq!(gitlab.short, "group/subgroup/repo");
        assert_eq!(gitlab.git_ref.as_deref(), Some("release"));
        assert_eq!(gitlab.subpath.as_deref(), Some("skills/demo"));

        let ssh = Source::parse("git@github.com:owner/repo.git#v2@demo").unwrap();
        assert_eq!(ssh.repo_url, "git@github.com:owner/repo.git");
        assert_eq!(ssh.short, "owner/repo");
        assert_eq!(ssh.git_ref.as_deref(), Some("v2"));
        assert_eq!(ssh.skill_filter.as_deref(), Some("demo"));
    }

    #[test]
    fn source_parse_rejects_unsafe_subpaths() {
        assert!(Source::parse("owner/repo/../secrets").is_err());
        assert!(Source::parse("owner/repo/./skills").is_err());
    }

    #[test]
    fn same_remote_url_ignores_tree_selection() {
        assert!(same_remote_url(
            "https://github.com/owner/repo/tree/main/skills/demo",
            "owner/repo"
        ));
    }

    #[test]
    fn cache_dir_name_sanitizes_ref_punctuation() {
        assert_eq!(
            cache_dir_name("owner/repo:feature\\demo"),
            "owner--repo-feature--demo"
        );
    }

    #[test]
    fn source_parse_empty_fails() {
        assert!(Source::parse("").is_err());
    }

    #[test]
    fn source_parse_whitespace_fails() {
        assert!(Source::parse("  ").is_err());
    }

    #[test]
    fn source_parse_invalid_format_fails() {
        assert!(Source::parse("not-valid").is_err());
        assert!(Source::parse("owner/").is_err());
        assert!(Source::parse("/repo").is_err());
    }
}
