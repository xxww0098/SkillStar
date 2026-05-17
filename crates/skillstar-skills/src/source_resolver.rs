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
}

impl Source {
    /// Parse any accepted input into a normalized `Source`.
    ///
    /// Accepts:
    /// - `owner/repo`
    /// - Full HTTPS GitHub URLs (with or without `.git` suffix)
    /// - URLs with trailing slashes
    ///
    /// Errors on empty or whitespace input.
    pub fn parse(input: &str) -> Result<Source> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Repository URL cannot be empty"));
        }

        if trimmed.to_lowercase().starts_with("https://") {
            let mut short = trimmed.to_string();

            if short.to_lowercase().starts_with("https://github.com/") {
                short = trimmed
                    .get("https://github.com/".len()..)
                    .unwrap_or(trimmed)
                    .to_string();
            }

            if short.ends_with(".git") {
                short = short[..short.len() - 4].to_string();
            }
            if short.ends_with('/') {
                short.pop();
            }

            let mut repo_url = trimmed.to_string();
            if repo_url.ends_with('/') {
                repo_url.pop();
            }
            if !repo_url.ends_with(".git") {
                repo_url.push_str(".git");
            }

            Ok(Source { repo_url, short })
        } else {
            let components: Vec<&str> = trimmed.split('/').collect();
            if components.len() != 2 || components[0].is_empty() || components[1].is_empty() {
                return Err(anyhow!(
                    "Invalid repository format. Use 'owner/repo' or a full GitHub URL."
                ));
            }

            let mut repo_name = components[1].to_string();
            if repo_name.ends_with(".git") {
                repo_name = repo_name[..repo_name.len() - 4].to_string();
            }

            let short = format!("{}/{}", components[0], repo_name);
            let repo_url = format!("https://github.com/{}.git", short);

            Ok(Source { repo_url, short })
        }
    }
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
    source.replace('/', "--")
}

pub fn same_remote_url(left: &str, right: &str) -> bool {
    normalize_remote_url(left) == normalize_remote_url(right)
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
