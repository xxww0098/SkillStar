//! Repository source resolution for SkillStar.
//!
//! Converts user-provided repository identifiers (shorthand, full URLs)
//! into normalised clone URLs and short display identifiers. Also provides
//! URL comparison helpers used during install and update flows.
//!
//! # Supported input formats
//!
//! | Input | Clone URL | Identifier |
//! |---|---|---|
//! | `owner/repo` | `https://github.com/owner/repo.git` | `owner/repo` |
//! | `https://github.com/owner/repo` | `https://github.com/owner/repo.git` | `owner/repo` |
//! | `https://github.com/owner/repo.git` | (as-is) | `owner/repo` |
//! | `https://other.host/path` | `https://other.host/path.git` | full URL without `.git` |
//!
//! # Example
//!
//! ```rust,ignore
//! let (clone_url, source) = normalize_repo_url("vercel-labs/skills")?;
//! assert_eq!(clone_url, "https://github.com/vercel-labs/skills.git");
//! assert_eq!(source, "vercel-labs/skills");
//! assert_eq!(cache_dir_name(&source), "vercel-labs--skills");
//! ```

use anyhow::{Result, anyhow};

// ── Input Classification ────────────────────────────────────────────

/// Classification of user-provided install input.
///
/// Determines how the install system should resolve the input into a
/// downloadable skill source.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum InputKind {
    /// Full HTTPS URL (e.g. `https://github.com/owner/repo`)
    Url(String),
    /// GitHub shorthand `owner/repo`
    OwnerRepo { owner: String, repo: String },
    /// Bare skill name (e.g. `adapt`) — needs marketplace resolution
    SkillName(String),
}

/// Classify user input to determine the resolution strategy.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(matches!(classify_input("adapt"), InputKind::SkillName(_)));
/// assert!(matches!(classify_input("owner/repo"), InputKind::OwnerRepo { .. }));
/// assert!(matches!(classify_input("https://github.com/o/r"), InputKind::Url(_)));
/// ```
#[allow(dead_code)]
pub fn classify_input(input: &str) -> InputKind {
    let trimmed = input.trim();

    // URLs
    if trimmed.to_lowercase().starts_with("https://")
        || trimmed.to_lowercase().starts_with("http://")
    {
        return InputKind::Url(trimmed.to_string());
    }

    // owner/repo (exactly two non-empty segments separated by /)
    if trimmed.contains('/') {
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            let repo = parts[1].trim_end_matches(".git").to_string();
            return InputKind::OwnerRepo {
                owner: parts[0].to_string(),
                repo,
            };
        }
        // 3+ segments without protocol — treat as invalid, but classify as URL attempt
        return InputKind::Url(trimmed.to_string());
    }

    // Single word — bare skill name
    InputKind::SkillName(trimmed.to_string())
}

// ── URL Normalization ───────────────────────────────────────────────

/// Normalize user input into a full clone URL and a short source identifier.
///
/// Returns `(clone_url, source_identifier)` where:
/// - `clone_url` is suitable for `git clone`
/// - `source_identifier` is a short display string (e.g. `owner/repo`)
///
/// # Errors
///
/// Returns an error if the input is empty, blank, or in an unrecognised format
/// (e.g. bare name without owner, or more than two path components).
pub fn normalize_repo_url(input: &str) -> Result<(String, String)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Repository URL cannot be empty"));
    }

    if trimmed.to_lowercase().starts_with("https://") {
        // Full HTTPS URL
        let mut source = trimmed.to_string();

        // Extract owner/repo from URL
        if source.to_lowercase().starts_with("https://github.com/") {
            source = trimmed
                .get("https://github.com/".len()..)
                .unwrap_or(trimmed)
                .to_string();
        }
        // else: Non-GitHub HTTPS URLs use the full URL as source

        // Clean up source
        if source.ends_with(".git") {
            source = source[..source.len() - 4].to_string();
        }
        if source.ends_with('/') {
            source.pop();
        }

        // Build clone URL
        let mut repo_url = trimmed.to_string();
        if repo_url.ends_with('/') {
            repo_url.pop();
        }
        if !repo_url.ends_with(".git") {
            repo_url.push_str(".git");
        }

        Ok((repo_url, source))
    } else {
        // owner/repo format
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

// ── Cache Key ───────────────────────────────────────────────────────

/// Derive a cache directory name from a source identifier.
///
/// Replaces `/` with `--` to produce a flat, filesystem-safe directory name.
///
/// `"vercel-labs/skills"` → `"vercel-labs--skills"`
#[allow(dead_code)]
pub fn cache_dir_name(source: &str) -> String {
    source.replace('/', "--")
}

// ── URL Comparison ──────────────────────────────────────────────────

/// Check whether two remote URLs refer to the same repository.
///
/// Normalises trailing `.git` suffix, trailing slashes, and case before
/// comparing, so `https://github.com/Foo/Bar.git` matches
/// `https://github.com/foo/bar`.
pub(crate) fn same_remote_url(left: &str, right: &str) -> bool {
    normalize_remote_url(left) == normalize_remote_url(right)
}

/// Strip `.git` suffix, trailing slashes, and lowercase for comparison.
pub(crate) fn normalize_remote_url(url: &str) -> String {
    url.trim()
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_input ──────────────────────────────────────────────

    #[test]
    fn classify_bare_name() {
        assert_eq!(
            classify_input("adapt"),
            InputKind::SkillName("adapt".into())
        );
        assert_eq!(
            classify_input("  my-skill  "),
            InputKind::SkillName("my-skill".into())
        );
    }

    #[test]
    fn classify_owner_repo() {
        assert_eq!(
            classify_input("vercel-labs/skills"),
            InputKind::OwnerRepo {
                owner: "vercel-labs".into(),
                repo: "skills".into()
            }
        );
    }

    #[test]
    fn classify_owner_repo_strips_git() {
        assert_eq!(
            classify_input("owner/repo.git"),
            InputKind::OwnerRepo {
                owner: "owner".into(),
                repo: "repo".into()
            }
        );
    }

    #[test]
    fn classify_https_url() {
        match classify_input("https://github.com/owner/repo") {
            InputKind::Url(u) => assert_eq!(u, "https://github.com/owner/repo"),
            other => panic!("expected Url, got {:?}", other),
        }
    }

    #[test]
    fn classify_http_url() {
        assert!(matches!(
            classify_input("http://example.com/foo"),
            InputKind::Url(_)
        ));
    }

    // ── normalize_repo_url ──────────────────────────────────────────

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
    fn normalize_invalid_format_fails() {
        assert!(normalize_repo_url("just-a-name").is_err());
        assert!(normalize_repo_url("a/b/c").is_err());
    }

    // ── cache_dir_name ──────────────────────────────────────────────

    #[test]
    fn cache_dir_name_conversion() {
        assert_eq!(cache_dir_name("vercel-labs/skills"), "vercel-labs--skills");
        assert_eq!(cache_dir_name("anthropics/courses"), "anthropics--courses");
    }

    // ── same_remote_url ─────────────────────────────────────────────

    #[test]
    fn same_remote_url_matches_with_git_suffix() {
        assert!(same_remote_url(
            "https://github.com/owner/repo.git",
            "https://github.com/owner/repo"
        ));
    }

    #[test]
    fn same_remote_url_case_insensitive() {
        assert!(same_remote_url(
            "https://github.com/Owner/Repo",
            "https://github.com/owner/repo"
        ));
    }

    #[test]
    fn same_remote_url_different_repos() {
        assert!(!same_remote_url(
            "https://github.com/owner/repo-a",
            "https://github.com/owner/repo-b"
        ));
    }
}
