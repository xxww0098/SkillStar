//! URL normalization for repository sources.
//!
//! Converts user-provided repository identifiers into normalized clone URLs
//! and short display identifiers.

use anyhow::{Result, anyhow};

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

pub fn normalize_repo_url(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Repository URL cannot be empty"));
    }

    if trimmed.to_lowercase().starts_with("https://") {
        let mut source = trimmed.to_string();

        if source.to_lowercase().starts_with("https://github.com/") {
            source = trimmed
                .get("https://github.com/".len()..)
                .unwrap_or(trimmed)
                .to_string();
        }

        if source.ends_with(".git") {
            source = source[..source.len() - 4].to_string();
        }
        if source.ends_with('/') {
            source.pop();
        }

        let mut repo_url = trimmed.to_string();
        if repo_url.ends_with('/') {
            repo_url.pop();
        }
        if !repo_url.ends_with(".git") {
            repo_url.push_str(".git");
        }

        Ok((repo_url, source))
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

        let source = format!("{}/{}", components[0], repo_name);
        let repo_url = format!("https://github.com/{}.git", source);

        Ok((repo_url, source))
    }
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
}
